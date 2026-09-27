"""Paired fixed-depth comparison for behavior-changing two-step route dedup.

Unlike run.py, node-count equality is not required. Complete move, score,
depth, legal count, nodes and time are retained for review.
"""
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


def endpoint(move):
    return None if move is None else (move['from'], move['to'], move['promoted'])


def summarize(rows):
    def ratio(key):
        return math.exp(sum(math.log(r['candidate'][key] / r['baseline'][key])
                            for r in rows) / len(rows))
    return {
        'pairs': len(rows),
        'same_complete_move': sum(r['baseline']['best'] == r['candidate']['best'] for r in rows),
        'same_endpoint': sum(endpoint(r['baseline']['best']) == endpoint(r['candidate']['best']) for r in rows),
        'same_score': sum(r['baseline']['score'] == r['candidate']['score'] for r in rows),
        'same_completed_depth': sum(r['baseline']['completed_depth'] == r['candidate']['completed_depth'] for r in rows),
        'changed_legal_count': sum(r['baseline']['legal'] != r['candidate']['legal'] for r in rows),
        'changed_nodes': sum(r['baseline']['nodes'] != r['candidate']['nodes'] for r in rows),
        'sum_nodes': {v: sum(r[v]['nodes'] for r in rows) for v in ('baseline', 'candidate')},
        'sum_q_nodes': {v: sum(r[v]['q_nodes'] for r in rows) for v in ('baseline', 'candidate')},
        'sum_cpu_ms': {v: sum(r[v]['cpu_ms'] for r in rows) for v in ('baseline', 'candidate')},
        'wall_geomean_ratio': ratio('ms'),
        'cpu_geomean_ratio': ratio('cpu_ms'),
        'mismatches': [dict(position=r['position'], model=r['model'], rep=r['rep'],
                            baseline_best=r['baseline']['best'], candidate_best=r['candidate']['best'],
                            baseline_score=r['baseline']['score'], candidate_score=r['candidate']['score'])
                       for r in rows if endpoint(r['baseline']['best']) != endpoint(r['candidate']['best'])
                       or r['baseline']['score'] != r['candidate']['score']],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for field in ('baseline', 'candidate', 'corpus', 'cases', 'out'):
        parser.add_argument('--' + field, type=Path, required=True)
    parser.add_argument('--cpu', type=int, default=min(os.sched_getaffinity(0)))
    parser.add_argument('--repeats', type=int, default=2)
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    (out / 'bin').mkdir()
    shutil.copyfile(args.corpus, out / 'corpus.json')
    cases = [{k: case[k] for k in ('position', 'model', 'depth')}
             for case in json.loads(args.cases.read_text())]
    plan = {'cases': cases, 'repeats': args.repeats, 'cpu': args.cpu,
            'source_sha256': hashlib.sha256((ROOT / 'src/game_state.rs').read_bytes()).hexdigest(),
            'binaries': {}, 'seed': 20260927}
    for name in ('baseline', 'candidate'):
        binary = getattr(args, name).resolve()
        (out / 'bin' / name).symlink_to(binary)
        plan['binaries'][name] = {'path': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}
    (out / 'plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    harness.OUT = out
    servers = {name: harness.Server(name, 'dedup', args.cpu) for name in ('baseline', 'candidate')}
    rows = []
    try:
        for case in cases:
            for name in ('baseline', 'candidate'):
                servers[name].request(case)
        print(f'Warmed {len(cases)} cases', flush=True)
        rng = random.Random(plan['seed'])
        starts = {(c['position'], c['model']): rng.randrange(2) for c in cases}
        with (out / 'pairs.jsonl').open('w') as file:
            for rep in range(args.repeats):
                order = cases.copy()
                rng.shuffle(order)
                for case in order:
                    names = ['baseline', 'candidate']
                    if (rep + starts[(case['position'], case['model'])]) % 2:
                        names.reverse()
                    measurements = {name: servers[name].request(case) for name in names}
                    row = dict(rep=rep, **case, order=names,
                               baseline=measurements['baseline'], candidate=measurements['candidate'])
                    file.write(json.dumps(row) + '\n')
                    file.flush()
                    rows.append(row)
                print(f'Repetition {rep + 1}/{args.repeats} done', flush=True)
    finally:
        for server in servers.values():
            server.close()
    summary = summarize(rows)
    (out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
