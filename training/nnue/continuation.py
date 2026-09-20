"""Continue immutable starter checkpoints sequentially, with one CPU thread per trainer."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import fcntl
import ctypes
from dataclasses import asdict
from plateau import Policy

HERE = Path(__file__).resolve().parent
WIDTHS = [512, 768, 1024, 1536, 2048]
CODE = ['continuation.py', 'train.py', 'plateau.py', 'quantized.py']
STOPPING = False


def digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''):
            h.update(chunk)
    return h.hexdigest()


def read(path):
    return json.loads(Path(path).read_text())


def atomic(path, value):
    path = Path(path)
    temp = path.with_suffix('.tmp')
    temp.write_text(json.dumps(value, indent=2) + '\n')
    temp.replace(path)


def identity(config):
    return hashlib.sha256(json.dumps(config, sort_keys=True).encode()).hexdigest()


def overlap(a, b):
    a, b = Path(a).resolve(), Path(b).resolve()
    return a.is_relative_to(b) or b.is_relative_to(a)


def load_config(path):
    c = read(path)
    if c.get('version') != 1 or not c.get('widths') or len(set(c['widths'])) != len(c['widths']):
        raise ValueError('Invalid training configuration')
    if any(w not in WIDTHS + [32] for w in c['widths']):
        raise ValueError('Unsupported training width')
    Policy(**c['policy'])
    import re
    if any(not re.fullmatch(r'[A-Za-z0-9_-]+', c[key]) for key in ('generation', 'seed_generation')):
        raise ValueError('Invalid generation')
    for key in ('dataset', 'base', 'checkpoints', 'seed_models', 'out'):
        c[key] = str(Path(c[key]).resolve())
    # Preserve a venv's Python symlink: resolving it would lose the venv environment.
    c['python'] = os.path.abspath(c['python'])
    if not os.access(c['python'], os.X_OK):
        raise ValueError('Missing training Python: ' + c['python'])
    for key in ('dataset', 'checkpoints', 'seed_models'):
        if overlap(c['out'], c[key]):
            raise ValueError('Training output must be separate from ' + key)
    if Path(c['base']).is_relative_to(c['out']):
        raise ValueError('Training output contains its material baseline')
    hashes = {name: digest(HERE / name) for name in CODE}
    if c.get('code_hashes', hashes) != hashes:
        raise ValueError('Training code changed; use the pinned code or a new output directory')
    c['code_hashes'] = hashes
    marker = Path(c['out']) / 'job.json'
    if marker.exists():
        if read(marker)['identity'] != identity(c):
            raise ValueError('Training output belongs to a different job')
    elif Path(c['out']).exists() and any(p.name != '.lock' for p in Path(c['out']).iterdir()):
        raise ValueError('Nonempty output has no training job identity')
    return c


def check_inputs(c):
    """Preflight before launching any tournament; map tensors without copying them."""
    import torch
    import numpy as np
    torch.set_num_threads(1)
    dataset = Path(c['dataset'])
    meta = read(dataset / 'dataset.json')
    for name, suffix in [('features', 'bin'), ('samples', 'jsonl'), ('schema', 'json')]:
        if digest(dataset / f'{name}.{suffix}') != meta[name + '_sha256']:
            raise ValueError('Dataset changed: ' + name)
    if digest(c['base']) != meta['baseline_sha256']:
        raise ValueError('Material baseline changed')
    base = read(c['base'])
    if base['weights'].get('nnue'):
        raise ValueError('Material baseline must not contain a network')
    dataset_id = identity(meta)
    schema = read(dataset / 'schema.json')
    samples = [json.loads(s) for s in (dataset / 'samples.jsonl').read_text().splitlines()]
    counts = {s: sum(x['split'] == s for x in samples) for s in ('train', 'validation')}
    if counts['train'] < 16 or counts['validation'] < 8:
        raise ValueError('Insufficient training/validation data')
    for width in c['widths']:
        key = str(width)
        path = Path(c['checkpoints']) / f'w{width}.training.pt'
        if digest(path) != c['seed_hashes'][key]:
            raise ValueError(f'Seed checkpoint changed: {width}')
        cp = torch.load(path, weights_only=True, mmap=True, map_location='cpu')
        recipe = c['recipes'][key]
        if cp['recipe'] != recipe or cp['dataset_id'] != dataset_id or cp['width'] != width:
            raise ValueError(f'Seed configuration mismatch: {width}')
        if recipe['train_count'] != counts['train'] or recipe['validation_count'] != counts['validation']:
            raise ValueError('Seed used a different sample subset')
        if tuple(cp['net']['embedding.weight'].shape) != (schema['features'], width):
            raise ValueError('Seed network dimensions differ')
        del cp
        model = Path(c['seed_models']) / f'NNUE_W{width}_{c["seed_generation"]}.json'
        if digest(model) != c['model_hashes'][key]:
            raise ValueError('Seed inference descriptor changed')
        weights = read(model)['weights']
        if weights['piece'] != base['weights']['piece']:
            raise ValueError('Seed material changed')
        d = weights['nnue']
        if d['width'] != width or d['feature_hash'] != schema['names_hash'] or digest(model.parent / d['file']) != d['sha256']:
            raise ValueError('Seed inference model changed')
    # Imports above also verify the actual training interpreter's dependencies.
    return dict(dataset_id=dataset_id, counts=counts, torch=torch.__version__, numpy=np.__version__)


def child_guard(parent):
    def setup():
        if ctypes.CDLL(None, use_errno=True).prctl(1, signal.SIGKILL, 0, 0, 0):
            raise OSError(ctypes.get_errno(), 'PR_SET_PDEATHSIG')
        if os.getppid() != parent:
            os.kill(os.getpid(), signal.SIGKILL)
    return setup


def stop_handler(*_):
    global STOPPING
    STOPPING = True


def run(c):
    out = Path(c['out'])
    out.mkdir(parents=True, exist_ok=True)
    with (out / '.lock').open('a+') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        # Recheck after taking the lock so competing launchers cannot initialize it twice.
        marker = out / 'job.json'
        if marker.exists() and read(marker)['identity'] != identity(c):
            raise ValueError('Training output belongs to a different job')
        atomic(marker, dict(identity=identity(c), config=c))
        progress = read(out / 'progress.json') if (out / 'progress.json').exists() else {'widths': {}}
        signal.signal(signal.SIGTERM, stop_handler)
        signal.signal(signal.SIGINT, stop_handler)
        lease_fd = int(os.environ['TAIKYOKU_TRAINING_LEASE_FD']) if 'TAIKYOKU_TRAINING_LEASE_FD' in os.environ else None
        for width in c['widths']:
            if STOPPING:
                raise InterruptedError('Training stopped before the next width')
            resume = out / f'w{width}.training.pt'
            recipe = c['recipes'][str(width)]
            command = [c['python'], str(HERE / 'train.py'), c['dataset'], '--base', c['base'],
                       '--out', c['out'], '--width', str(width), '--threads', '1', '--plateau',
                       '--generation', c['generation'], '--prune-exports']
            for flag, key in [('seed', 'seed'), ('batch', 'batch'), ('embedding-lr', 'embedding_lr'), ('head-lr', 'head_lr')]:
                command += ['--' + flag, str(recipe[key])]
            for key, value in c['policy'].items():
                command += ['--epochs' if key == 'max_epochs' else '--' + key.replace('_', '-'), str(value)]
            if resume.exists():
                command += ['--resume']
            else:
                seed = Path(c['checkpoints']) / f'w{width}.training.pt'
                if digest(seed) != c['seed_hashes'][str(width)]:
                    raise ValueError('Seed checkpoint changed after preflight')
                command += ['--resume-from', str(seed), '--seed-model', str(Path(c['seed_models']) / f'NNUE_W{width}_{c["seed_generation"]}.json')]
            atomic(out / 'progress.json', dict(progress, state='training', current_width=width, updated=time.time()))
            env = dict(os.environ, OMP_NUM_THREADS='1', MKL_NUM_THREADS='1', OPENBLAS_NUM_THREADS='1', NUMEXPR_NUM_THREADS='1')
            with (out / f'w{width}.log').open('ab') as log:
                child = subprocess.Popen(command, env=env, stdout=log, stderr=log, start_new_session=True,
                                         pass_fds=() if lease_fd is None else (lease_fd,), preexec_fn=child_guard(os.getpid()))
                try:
                    while child.poll() is None and not STOPPING:
                        time.sleep(.2)
                finally:
                    if child.poll() is None:
                        child.terminate()
                        try:
                            child.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            os.killpg(child.pid, signal.SIGKILL)
                            child.wait()
                if STOPPING:
                    atomic(out / 'progress.json', dict(progress, state='interrupted', current_width=width, updated=time.time()))
                    raise InterruptedError('Training interrupted; completed epochs retained')
                if child.returncode:
                    atomic(out / 'progress.json', dict(progress, state='failed', current_width=width, exit=child.returncode, updated=time.time()))
                    raise RuntimeError(f'Width {width} failed ({child.returncode}); see {out / f"w{width}.log"}')
            status = read(out / f'w{width}.status.json')
            if status['state'] != 'completed':
                raise RuntimeError('Trainer exited without a completion status')
            progress['widths'][str(width)] = status
            atomic(out / 'progress.json', dict(progress, state='training', updated=time.time()))
        atomic(out / 'progress.json', dict(progress, state='completed', updated=time.time()))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest='action', required=True)
    for action in ('check', 'run'):
        sub.add_parser(action).add_argument('config', type=Path)
    make = sub.add_parser('prepare')
    for key in ('dataset', 'base', 'checkpoints', 'seed-models', 'out', 'python', 'config'):
        make.add_argument('--' + key, required=True)
    make.add_argument('--widths', nargs='+', type=int, default=WIDTHS)
    make.add_argument('--generation', default='v2')
    make.add_argument('--seed-generation', default='v1')
    a = p.parse_args()
    if a.action == 'prepare':
        import torch
        config_path = Path(a.config)
        if config_path.exists():
            raise ValueError('Configuration already exists')
        c = {key: os.path.abspath(getattr(a, key)) for key in ('dataset', 'base', 'checkpoints', 'seed_models', 'out', 'python')}
        c.update(version=1, widths=a.widths, generation=a.generation, seed_generation=a.seed_generation,
                 policy=asdict(Policy()), seed_hashes={}, model_hashes={}, recipes={})
        for width in a.widths:
            seed = Path(c['checkpoints']) / f'w{width}.training.pt'
            c['seed_hashes'][str(width)] = digest(seed)
            cp = torch.load(seed, mmap=True, weights_only=True, map_location='cpu')
            c['recipes'][str(width)] = cp['recipe']
            del cp
            c['model_hashes'][str(width)] = digest(Path(c['seed_models']) / f'NNUE_W{width}_{a.seed_generation}.json')
        config_path.parent.mkdir(parents=True, exist_ok=True)
        atomic(config_path, c)
        print(config_path)
    elif a.action == 'check':
        c = load_config(a.config)
        check_inputs(c)
        print(json.dumps(c))
    else:
        run(load_config(a.config))


if __name__ == '__main__':
    try:
        main()
    except Exception as e:
        print(f'nnue continuation: {e}', file=sys.stderr, flush=True)
        sys.exit(130 if isinstance(e, InterruptedError) else 1)
