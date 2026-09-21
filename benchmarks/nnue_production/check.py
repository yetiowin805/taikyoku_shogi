#!/usr/bin/env python3
"""Compare production NNUE with content-pinned baseline/prototype executables."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import random
import statistics
import subprocess
import time

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--root', type=Path, required=True)
p.add_argument('--out', type=Path, required=True)
p.add_argument('--candidate', type=Path, required=True)
p.add_argument('--phase', choices=['verify', 'timed'], required=True)
a = p.parse_args()
a.out.mkdir(parents=True, exist_ok=True)
prior = a.root / 'data/nnue-post-merge-ablation-20260920'
positions = json.loads((a.root / 'data/nnue-speed-probe-20260920/manifest.json').read_text())['positions']
models = {f'W{w}': a.root / f'data/nnue-admission-v2/NNUE_W{w}_v2.json' for w in [512, 768, 1024, 1536, 2048]}
for name in ['BASE_C2S2_A40_Lmate', 'C2K50A1']:
    models[name] = a.root / f'data/nnue-speed-probe-20260920/models/{name}.json'
if a.phase == 'timed':
    models = {k: v for k, v in models.items() if k in ['W512', 'W2048']}
reference = prior / 'bin' / ('main' if a.phase == 'verify' else 'probes')

def sha(path):
    with open(path, 'rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()

old = json.loads((prior / 'timed-manifest.json').read_text())
assert sha(reference) == old['binary_sha256']['stock' if a.phase == 'verify' else 'probes']
for pos in positions:
    assert sha(pos['game']) == old['game_sha256'][pos['game']]
rng = random.Random(9212026)
blocks = [(model, pos, rng.sample(['reference', 'candidate'], 2)) for model in models for pos in positions]
order = []
for rep in range(2 if a.phase == 'timed' else 1):
    for model, pos, variants in rng.sample(blocks, len(blocks)):
        for variant in variants if rep == 0 else variants[::-1]:
            order.append((rep, model, pos, variant))
manifest = {'revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
            'diff': subprocess.check_output(['git', 'diff'], text=True), 'phase': a.phase,
            'source_sha256': {str(path): sha(path) for path in [*Path('src/nnue').glob('*.rs'), Path('src/search.rs'), Path('src/game_state.rs'), Path('examples/nnue_speed_check.rs'), Path(__file__)]},
            'binary_sha256': {'reference': sha(reference), 'candidate': sha(a.candidate)},
            'game_sha256': {x['game']: sha(x['game']) for x in positions},
            'models': {name: {'path': str(path), 'sha256': sha(path), 'nnue': json.loads(path.read_text())['weights'].get('nnue')} for name, path in models.items()},
            'cpu': 0, 'rustflags': '-C target-cpu=native', 'compiler': subprocess.check_output(['rustc', '-Vv'], text=True),
            'order': order, 'warmup_depth': 1, 'reference_main': '05b6ec7', 'timed_reference': 'full bundle, not unoptimized main'}
with (a.out / f'{a.phase}-manifest.json').open('x') as f:
    json.dump(manifest, f, indent=2)
rows = []
with (a.out / f'{a.phase}.jsonl').open('x') as f:
    for i, (rep, model, pos, variant) in enumerate(order):
        env = {k: v for k, v in os.environ.items() if not k.startswith('NNUE_')}
        env.update(NNUE_ABLATION_WARMUP='1', NNUE_SPEED_PROBE='both' if a.phase == 'timed' else 'baseline', NNUE_FOLLOWUP='packed,royal' if a.phase == 'timed' else '')
        limit, depth = (30000, 2) if a.phase == 'verify' else (3000, 8)
        binary = reference if variant == 'reference' else a.candidate
        command = ['taskset', '-c', '0', str(binary), str(models[model]), pos['game'], str(pos['ply']), str(limit), str(depth)]
        if variant == 'reference':
            command.append('search')
        start = time.monotonic()
        run = subprocess.run(command, env=env, text=True, capture_output=True, timeout=90)
        (a.out / f'{a.phase}-{i:03d}.stderr').write_text(run.stderr)
        if run.returncode:
            raise RuntimeError(run.stderr)
        result = json.loads(run.stdout)
        assert result['legal']
        if a.phase == 'verify':
            assert not result['aborted'] and result['depth'] == depth, (model, pos, variant)
        row = dict(rep=rep, model=model, position=pos['name'], variant=variant, result=result, wall_seconds=time.monotonic()-start)
        f.write(json.dumps(row)+'\n'); f.flush(); rows.append(row)
        print(a.phase, f'{i+1}/{len(order)}', model, pos['name'], variant, 'depth', result['depth'], flush=True)
for row in rows:
    if row['variant'] != 'candidate':
        continue
    ref = next(r for r in rows if r['rep'] == row['rep'] and r['model'] == row['model'] and r['position'] == row['position'] and r['variant'] == 'reference')
    if a.phase == 'verify':
        for key in ['depth', 'score', 'static_score', 'best', 'root_lines', 'nodes', 'qnodes']:
            assert row['result'][key] == ref['result'][key], (row['model'], row['position'], key)
if a.phase == 'verify':
    summary = {'equal_depth_pairs': len(rows)//2, 'all_signatures_match': True}
else:
    summary = {}
    for model in models:
        ratios = []
        for row in rows:
            if row['variant'] != 'candidate' or row['model'] != model:
                continue
            ref = next(r for r in rows if r['variant'] == 'reference' and r['model'] == model and r['position'] == row['position'] and r['rep'] == row['rep'])
            nps = lambda r: r['result']['nodes']/r['result']['elapsed_ns']
            ratios.append(nps(row)/nps(ref))
        summary[model] = {'nps_ratio_to_tested_prototype': math.exp(statistics.mean(map(math.log, ratios))), 'pairs': len(ratios)}
(a.out / f'{a.phase}-summary.json').write_text(json.dumps(summary, indent=2)+'\n')
print(json.dumps(summary), flush=True)
