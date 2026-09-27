"""Paired timed-search experiment; records move/depth/clock changes."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import random
import shutil
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'benchmarks/search_speed_20260920'))
import run as harness


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ('baseline', 'candidate', 'corpus', 'cases', 'out'):
        p.add_argument('--' + name, type=Path, required=True)
    p.add_argument('--cpu', type=int, default=min(os.sched_getaffinity(0)))
    args = p.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    (out / 'bin').mkdir()
    shutil.copyfile(args.corpus, out / 'corpus.json')
    original = json.loads(args.cases.read_text())
    cases = [dict(position=c['position'], model=c['model'], depth=8, budget_ms=1000)
             for c in original]
    cases += [dict(position=c['position'], model=c['model'], depth=8, budget_ms=3000)
              for c in original if c['position'] % 2 == 0]
    rng = random.Random(20260927)
    rng.shuffle(cases)
    plan = {'cases': cases, 'cpu': args.cpu, 'binaries': {},
            'source_sha256': hashlib.sha256((ROOT / 'src/search.rs').read_bytes()).hexdigest(),
            'seed': 20260927}
    for name in ('baseline', 'candidate'):
        binary = getattr(args, name).resolve()
        (out / 'bin' / name).symlink_to(binary)
        plan['binaries'][name] = {'path': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}
    (out / 'plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    harness.OUT = out
    servers = {name: harness.Server(name, 'timed', args.cpu) for name in ('baseline', 'candidate')}
    rows = []
    try:
        # Warm every model/position at a short, fixed depth before timing.
        for c in original:
            q = {k: c[k] for k in ('position', 'model', 'depth')}
            for name in ('baseline', 'candidate'):
                servers[name].request(q)
        with (out / 'pairs.jsonl').open('w') as file:
            for i, case in enumerate(cases):
                names = ['baseline', 'candidate']
                if i % 2:
                    names.reverse()
                measurements = {name: servers[name].request(case, case['budget_ms']) for name in names}
                row = {**case, 'order': names, 'measurements': measurements}
                file.write(json.dumps(row) + '\n')
                file.flush()
                rows.append(row)
                if (i + 1) % 8 == 0:
                    print(f'{i + 1}/{len(cases)} timed pairs', flush=True)
    finally:
        for server in servers.values():
            server.close()
    summary = {}
    for budget in (1000, 3000):
        subset = [r for r in rows if r['budget_ms'] == budget]
        def b(row): return row['measurements']['baseline']
        def c(row): return row['measurements']['candidate']
        summary[str(budget)] = {
            'pairs': len(subset),
            'same_complete_move': sum(b(r)['best'] == c(r)['best'] for r in subset),
            'same_score': sum(b(r)['score'] == c(r)['score'] for r in subset),
            'same_depth': sum(b(r)['completed_depth'] == c(r)['completed_depth'] for r in subset),
            'sum_wall_ms': {n: sum(r['measurements'][n]['ms'] for r in subset) for n in ('baseline', 'candidate')},
            'wall_geomean_ratio': math.exp(sum(math.log(c(r)['ms'] / b(r)['ms']) for r in subset) / len(subset)),
            'changed': [dict(position=r['position'], model=r['model'],
                             baseline_best=b(r)['best'], candidate_best=c(r)['best'],
                             baseline_score=b(r)['score'], candidate_score=c(r)['score'],
                             baseline_depth=b(r)['completed_depth'], candidate_depth=c(r)['completed_depth'])
                        for r in subset if b(r)['best'] != c(r)['best'] or b(r)['score'] != c(r)['score']],
        }
    (out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
