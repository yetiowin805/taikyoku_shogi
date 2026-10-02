#!/usr/bin/env python3
"""Paired real-network searches; replay/loading excluded, all search setup included."""
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


def sha(path):
    with open(path, 'rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ('baseline', 'candidate', 'corpus', 'cases', 'out'):
        p.add_argument('--' + name, type=Path, required=True)
    p.add_argument('--cpu', type=int, default=2)
    p.add_argument('--repeats', type=int, default=2)
    p.add_argument('--models', type=int, nargs='+')
    p.add_argument('--positions', type=int, nargs='+')
    p.add_argument('--sequential', action='store_true', help='Keep adjacent game prefixes together')
    a = p.parse_args()
    if a.repeats < 1:
        p.error('repeats must be positive')
    a.out.mkdir(parents=True, exist_ok=False)
    (a.out / 'bin').mkdir()
    shutil.copyfile(a.corpus, a.out / 'corpus.json')
    corpus = json.loads(a.corpus.read_text())
    cases = [{k: c[k] for k in ('position', 'model', 'depth')}
             for c in json.loads(a.cases.read_text())
             if (a.models is None or c['model'] in a.models)
             and (a.positions is None or c['position'] in a.positions)]
    assert cases
    for entry in corpus['positions']:
        assert sha(entry['game']) == entry['sha256']
    for model in {case['model'] for case in cases}:
        entry = corpus['models'][model]
        assert sha(entry['path']) == entry['sha256']
    builds = {}
    for name in ('baseline', 'candidate'):
        binary = getattr(a, name).resolve()
        (a.out / 'bin' / name).symlink_to(binary)
        builds[name] = {'path': str(binary), 'sha256': sha(binary)}
    plan = dict(cases=cases, cpu=a.cpu, repeats=a.repeats, sequential=a.sequential,
                builds=builds, corpus=corpus, seed=20261002,
                compiler=subprocess.check_output(['rustc', '-Vv'], text=True),
                machine=subprocess.check_output(['lscpu'], text=True),
                load_before=os.getloadavg(),
                metric='Geometric mean of per-case candidate/baseline wall-time ratios')
    (a.out / 'plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    harness.OUT = a.out
    servers = {}
    rows = []
    try:
        servers = {name: harness.Server(name, 'paired', a.cpu) for name in builds}
        for case in cases:
            harness.compare(servers['baseline'].request(case), servers['candidate'].request(case))
        print(f'Warmup: {len(cases)} exact matches', flush=True)
        rng = random.Random(plan['seed'])
        starts = {(c['position'], c['model']): rng.randrange(2) for c in cases}
        with (a.out / 'pairs.jsonl').open('w') as f:
            for rep in range(a.repeats):
                order = cases.copy()
                if not a.sequential:
                    rng.shuffle(order)
                for case in order:
                    names = list(builds)
                    if (rep + starts[(case['position'], case['model'])]) % 2:
                        names.reverse()
                    result = {n: servers[n].request(case) for n in names}
                    harness.compare(result['baseline'], result['candidate'])
                    row = dict(rep=rep, **case, order=names, measurements=result)
                    f.write(json.dumps(row) + '\n')
                    f.flush()
                    rows.append(row)
                print(f'Repetition {rep + 1}: exact matches', flush=True)
    finally:
        for server in servers.values():
            server.close()
    def summarize(group):
        ratios = [r['measurements']['candidate']['ms'] / r['measurements']['baseline']['ms'] for r in group]
        total = {n: sum(r['measurements'][n]['ms'] for r in group) for n in builds}
        return dict(pairs=len(group), time_ratio=math.exp(sum(map(math.log, ratios)) / len(ratios)),
                    total_ms=total, total_time_ratio=total['candidate']/total['baseline'],
                    peak_rss_kib={n: max(r['measurements'][n]['peak_rss_kib'] for r in group) for n in builds})
    summary = dict(all=summarize(rows),
                   by_model={corpus['models'][m]['name']: summarize([r for r in rows if r['model'] == m])
                             for m in sorted({r['model'] for r in rows})},
                   by_position={corpus['positions'][i]['name']: summarize([r for r in rows if r['position'] == i])
                                for i in sorted({r['position'] for r in rows})},
                   all_signatures_match=True, load_after=os.getloadavg())
    (a.out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary['by_model'], indent=2), flush=True)


if __name__ == '__main__':
    main()
