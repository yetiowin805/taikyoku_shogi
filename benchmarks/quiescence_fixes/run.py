#!/usr/bin/env python3
"""Small sequential correctness/cost smoke test, not a playing-strength benchmark."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import subprocess
import time

HERE = Path(__file__).resolve().parent


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--baseline', required=True, type=Path)
    ap.add_argument('--fixed', required=True, type=Path)
    ap.add_argument('--data-root', required=True, type=Path)
    ap.add_argument('--cpu', type=int, default=2)
    ap.add_argument('--output', type=Path, default=HERE / 'results.json')
    args = ap.parse_args()
    corpus = json.loads((HERE / 'corpus.json').read_text())
    model = args.data_root / corpus['model']
    assert sha(model) == corpus['model_sha256']
    assert sha(args.data_root / corpus['model_blob']) == corpus['model_blob_sha256']
    for p in corpus['positions']:
        assert sha(args.data_root / p['game']) == p['game_sha256']
    binaries = {'baseline': args.baseline.resolve(), 'fixed': args.fixed.resolve()}
    output = dict(cpu=args.cpu, corpus=corpus, binaries={k: dict(path=str(v), sha256=sha(v))
        for k, v in binaries.items()}, runs=[])
    assert output['binaries']['baseline']['sha256'] != output['binaries']['fixed']['sha256'], \
        'Baseline and fixed binaries must be distinct builds'
    cases = [(p, mode, variant, rep) for p in corpus['positions']
             for mode in (['depth2', 'timed3s'] if p['id'].startswith('target') else ['depth2'])
             for variant in binaries for rep in range(2)]
    random.Random(20261008).shuffle(cases)
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(('TAIKYOKU_AB_', 'TACTICAL_'))}
    env['TAIKYOKU_AB_QDEPTH'] = '2'
    for p, mode, variant, rep in cases:
        depth, limit = (2, 30000) if mode == 'depth2' else (64, 3000)
        command = ['nice', '-n', '10', 'taskset', '-c', str(args.cpu), str(binaries[variant]),
                   str(args.data_root / p['game']), str(p['ply']), str(model), str(depth), str(limit)]
        started = time.monotonic()
        result = subprocess.run(command, env=env, text=True, capture_output=True, timeout=60)
        events = [json.loads(line) for line in result.stdout.splitlines() if line.startswith('{')]
        output['runs'].append(dict(position=p['id'], mode=mode, variant=variant, repetition=rep,
            wall_seconds=time.monotonic()-started, returncode=result.returncode,
            iterations=events, stderr=result.stderr))
        args.output.write_text(json.dumps(output, indent=2)+'\n')
        assert result.returncode == 0 and events, result.stderr
        if mode == 'depth2':
            assert events[-1]['completed_depth'] == 2, 'Incomplete depth is not equal-depth timing'
        print(p['id'], mode, variant, events[-1], flush=True)


if __name__ == '__main__':
    main()
