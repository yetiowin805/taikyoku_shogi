#!/usr/bin/env python3
"""Exercise a PGO-instrumented benchmark; this does not train NNUE weights."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('binary', 'corpus', 'out'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--positions', nargs='+', type=int, required=True)
    parser.add_argument('--models', nargs='+', type=int, required=True)
    parser.add_argument('--repeats', type=int, default=20)
    parser.add_argument('--depth', type=int, default=2)
    parser.add_argument('--cpu', type=int, default=min(os.sched_getaffinity(0)))
    args = parser.parse_args()
    if min(args.repeats, args.depth) < 1:
        parser.error('repeats and depth must be positive')
    if args.cpu not in os.sched_getaffinity(0):
        parser.error('selected CPU is outside the allowed affinity mask')
    corpus = json.loads(args.corpus.read_text())
    inputs = {entry['game']: entry['sha256']
              for entry in [corpus['positions'][i] for i in args.positions]}
    inputs.update({corpus['models'][i]['path']: corpus['models'][i]['sha256']
                   for i in args.models})
    for path, expected in inputs.items():
        with open(path, 'rb') as file:
            if hashlib.file_digest(file, 'sha256').hexdigest() != expected:
                raise RuntimeError(f'profile input changed: {path}')
    args.out.mkdir(parents=True, exist_ok=False)
    profiles = args.out.resolve() / 'profiles'
    profiles.mkdir()
    requests = [dict(position=p, model=m, depth=args.depth)
                for m in args.models for _ in range(args.repeats) for p in args.positions]
    plan = dict(requests=requests, cpu=args.cpu, corpus=corpus,
                binary=str(args.binary.resolve()),
                binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest())
    (args.out / 'plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    env = dict(os.environ, LLVM_PROFILE_FILE=str(profiles / 'train-%p-%m.profraw'))
    with (args.out / 'stderr.log').open('w') as err, (args.out / 'searches.jsonl').open('w') as out:
        proc = subprocess.Popen(['taskset', '-c', str(args.cpu), str(args.binary.resolve()),
                                 str(args.corpus.resolve())], env=env, stdin=subprocess.PIPE,
                                stdout=subprocess.PIPE, stderr=err, text=True)
        try:
            for request in requests:
                proc.stdin.write(json.dumps(request) + '\n')
                proc.stdin.flush()
                if not select.select([proc.stdout], [], [], 120)[0]:
                    raise TimeoutError(request)
                line = proc.stdout.readline()
                if not line:
                    raise RuntimeError('instrumented engine exited; inspect stderr.log')
                result = json.loads(line)
                if result['aborted']:
                    raise RuntimeError(f'profile search aborted: {request}')
                out.write(json.dumps(result) + '\n')
            proc.stdin.close()
            if proc.wait(timeout=30):
                raise RuntimeError('instrumented engine failed; inspect stderr.log')
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.wait()
    if not list(profiles.glob('*.profraw')):
        raise RuntimeError('no profiles: build with -C profile-generate first')
    print(f'{len(requests)} searches; profiles in {profiles}')


if __name__ == '__main__':
    main()
