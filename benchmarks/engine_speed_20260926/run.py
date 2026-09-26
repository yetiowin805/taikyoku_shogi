"""Small paired parity/timing check using the existing search-speed JSONL harness.

Build benchmarks/search_speed_20260920/harness.rs as an example at each revision
and pass the two resulting binaries. Inputs retain full game prefixes/models.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import random
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'benchmarks/search_speed_20260920'))
import run as harness


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ('baseline', 'candidate', 'corpus', 'cases', 'out'):
        p.add_argument('--' + name, type=Path, required=True)
    p.add_argument('--cpu', type=int, default=min(os.sched_getaffinity(0)))
    p.add_argument('--repeats', type=int, default=2)
    p.add_argument('--baseline-rev', default='c34366b')
    args = p.parse_args()
    if args.repeats < 1:
        p.error('--repeats must be positive')
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    (out / 'bin').mkdir()
    shutil.copyfile(args.corpus, out / 'corpus.json')
    cases = [{k: c[k] for k in ('position', 'model', 'depth')}
             for c in json.loads(args.cases.read_text())]
    builds = {}
    for name in ('baseline', 'candidate'):
        binary = getattr(args, name).resolve()
        (out / 'bin' / name).symlink_to(binary)
        builds[name] = {'path': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}
    plan = dict(cases=cases, repeats=args.repeats, cpu=args.cpu, builds=builds,
                baseline_revision=args.baseline_rev, candidate_revision=subprocess.check_output(
                    ['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                candidate_source_dirty=bool(subprocess.check_output(
                    ['git', 'diff', '--name-only', 'HEAD', '--', 'src'], cwd=ROOT, text=True).strip()),
                sources={str(f.relative_to(ROOT)): hashlib.sha256(f.read_bytes()).hexdigest()
                         for f in (ROOT / 'src').rglob('*.rs')},
                corpus=json.loads(args.corpus.read_text()),
                compiler=subprocess.check_output(['rustc', '-Vv'], text=True),
                seed=20260926)
    (out / 'plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    harness.OUT = out
    servers = {}
    rows = []
    try:
        for name in builds:
            servers[name] = harness.Server(name, 'paired', args.cpu)
        for case in cases:
            harness.compare(servers['baseline'].request(case), servers['candidate'].request(case))
        print(f'Warmup: {len(cases)} exact signature matches', flush=True)
        rng = random.Random(plan['seed'])
        starts = {(c['position'], c['model']): rng.randrange(2) for c in cases}
        with (out / 'pairs.jsonl').open('w') as f:
            for rep in range(args.repeats):
                order = cases.copy()
                rng.shuffle(order)
                for case in order:
                    names = list(builds)
                    if (rep + starts[(case['position'], case['model'])]) % 2:
                        names.reverse()
                    results = {n: servers[n].request(case) for n in names}
                    harness.compare(results['baseline'], results['candidate'])
                    row = dict(rep=rep, **case, order=names, measurements=results)
                    f.write(json.dumps(row) + '\n')
                    f.flush()
                    rows.append(row)
                print(f'Repetition {rep+1}/{args.repeats}: all signatures match', flush=True)
    finally:
        for server in servers.values():
            server.close()
    def ratio(group):
        return math.exp(sum(math.log(r['measurements']['candidate']['ms'] /
                                    r['measurements']['baseline']['ms']) for r in group) / len(group))
    summary = {'pairs': len(rows), 'candidate_over_baseline_geomean': ratio(rows),
               'by_model': {m: ratio([r for r in rows if r['model'] == m]) for m in sorted({r['model'] for r in rows})},
               'by_position': {pos: ratio([r for r in rows if r['position'] == pos]) for pos in sorted({r['position'] for r in rows})},
               'interpretation': 'Short smoke measurements, not a held-out speedup or strength study.'}
    (out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
