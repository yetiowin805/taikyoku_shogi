"""Single-CPU training sidecar using the tournament's existing game-boundary lease."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time


def prepare(coordinator, config_path, run, previous=None, dry_run=False, entrants=None):
    """Validate dependencies before launch and freeze the training program/config."""
    a = coordinator
    if config_path is None:
        if not previous:
            raise ValueError('Training mode requires --training-config')
        for filename, sha in previous['code_hashes'].items():
            if a.file_digest(Path(previous['code_dir']) / filename) != sha:
                raise ValueError('Pinned training code changed')
        source = Path(previous['config'])
        if a.file_digest(source) != previous['config_sha256']:
            raise ValueError('Pinned training configuration changed')
        script = Path(previous['code_dir']) / 'continuation.py'
    else:
        source = a.absolute(config_path)
        script = a.ROOT / 'training/nnue/continuation.py'
    raw = a.read(source)
    interpreter = os.path.abspath(a.absolute(raw['python']))
    result = subprocess.run([interpreter, str(script), 'check', str(source)], cwd=a.ROOT,
                            capture_output=True, text=True, timeout=600)
    if result.returncode:
        raise ValueError('Training preflight failed: ' + result.stderr.strip())
    config = json.loads(result.stdout)
    out = Path(config['out'])
    # The trainer may never publish into this run or any model directory it uses.
    protected = [run]
    if entrants is None:
        entrants = a.read(run / 'state.json')['entrants'] if (run / 'state.json').exists() else []
    for entrant in entrants:
        path = a.absolute(entrant['model']).resolve()
        protected.append(path.parent)
        nnue = a.read(path).get('weights', {}).get('nnue')
        if nnue:
            protected.append((path.parent / nnue['file']).resolve().parent)
    for path in protected:
        if out.is_relative_to(path) or path.is_relative_to(out):
            raise ValueError(f'Training output overlaps a live run/model directory: {path}')
    if dry_run:
        return config
    control = run / 'training'
    control.mkdir(parents=True, exist_ok=True)
    hashes = config['code_hashes']
    code_id = hashlib.sha256(json.dumps(hashes, sort_keys=True).encode()).hexdigest()
    target = control / ('code-' + code_id)
    target.mkdir(exist_ok=True)
    for filename, sha in hashes.items():
        data = (script.parent / filename).read_bytes()
        if a.digest(data) != sha:
            raise ValueError('Training code changed during preflight')
        dest = target / filename
        if dest.exists():
            if a.file_digest(dest) != sha:
                raise ValueError('Corrupt pinned training code')
        else:
            dest.write_bytes(data)
    saved = control / 'config.json'
    a.atomic(saved, config)
    return dict(config=str(saved), config_sha256=a.file_digest(saved), code_dir=str(target),
                code_hashes=hashes, python=interpreter, out=str(out),
                command=[interpreter, str(target / 'continuation.py'), 'run', str(saved)])


def run(coordinator, config):
    a = coordinator
    run_dir = Path(config['run'])
    control = run_dir / 'analysis'
    status = run_dir / 'training/status.json'
    training = config['training']
    os.sched_setaffinity(0, {config['cpus'][3]})
    if a.file_digest(training['config']) != training['config_sha256']:
        raise ValueError('Pinned training configuration changed')
    for name, sha in training['code_hashes'].items():
        if a.file_digest(Path(training['code_dir']) / name) != sha:
            raise ValueError('Pinned training code changed')
    request = control / 'analysis.request'  # Existing Rust admission protocol; shared by either sidecar.
    child = None
    try:
        request.touch()
        a.atomic(status, dict(state='waiting_for_cpu', cpu=config['cpus'][3], updated=time.time()))
        with (control / 'shared.lock').open('a+') as lease:
            while not a.STOPPING:
                try:
                    fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    break
                except BlockingIOError:
                    time.sleep(.2)
            if a.STOPPING:
                raise InterruptedError('Training stopped while waiting for the CPU')
            env = dict(os.environ, OMP_NUM_THREADS='1', MKL_NUM_THREADS='1', OPENBLAS_NUM_THREADS='1',
                       NUMEXPR_NUM_THREADS='1', TAIKYOKU_TRAINING_LEASE_FD=str(lease.fileno()))
            child = subprocess.Popen(training['command'], cwd=a.ROOT, env=env,
                                     pass_fds=(lease.fileno(),), start_new_session=True,
                                     preexec_fn=a.parent_death_guard(os.getpid()))
            a.atomic(status, dict(state='training', cpu=config['cpus'][3], pid=child.pid, out=training['out'], updated=time.time()))
            try:
                while child.poll() is None and not a.STOPPING:
                    time.sleep(.2)
            finally:
                if child.poll() is None:
                    child.terminate()
                    try:
                        child.wait(timeout=15)
                    except subprocess.TimeoutExpired:
                        os.killpg(child.pid, signal.SIGKILL)
                        child.wait()
            if a.STOPPING:
                raise InterruptedError('Training interrupted; completed epochs retained')
            if child.returncode:
                raise RuntimeError(f'Training failed ({child.returncode}); see training/trainer.log')
        a.atomic(status, dict(state='completed', cpu=config['cpus'][3], out=training['out'], updated=time.time()))
    except Exception as e:
        a.atomic(status, dict(state='interrupted' if isinstance(e, InterruptedError) else 'failed',
                              error=str(e), out=training['out'], updated=time.time()))
        raise
    finally:
        request.unlink(missing_ok=True)
