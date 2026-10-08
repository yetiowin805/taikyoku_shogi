#!/usr/bin/env python3
"""Compact, reproducible summary of run.py records (retain raw logs separately)."""
import collections
import json
from pathlib import Path
import statistics

HERE = Path(__file__).resolve().parent


def collect(root):
    rows = []
    manifests = {}
    for directory in sorted(root.iterdir()):
        if not directory.is_dir() or not (directory / 'manifest.json').exists():
            continue
        manifests[directory.name] = json.loads((directory / 'manifest.json').read_text())
        for path in sorted(directory.glob('*.json')):
            record = json.loads(path.read_text())
            if 'position' not in record or 'variant' not in record:
                continue
            fields = ['position', 'variant', 'repetition', 'group', 'mode', 'requested_depth',
                      'search_limit_ms', 'returncode', 'external_timeout', 'wall_seconds',
                      'completed_depth', 'fixed_depth_complete', 'best_move', 'score_black',
                      'nodes_last_iteration', 'search_elapsed_ms', 'move_category', 'env']
            row = {key: record.get(key) for key in fields}
            final = record.get('completion', {})
            row.update(phase=directory.name, nodes_total=final.get('nodes'),
                       engine_elapsed_ms=final.get('elapsed_ms'),
                       q_nodes_total=record.get('q_nodes_final'), aborted=final.get('aborted'),
                       tactical=final.get('tactical'), pv=record.get('pv_trace', []),
                       iterations=[{k: e.get(k) for k in ['completed_depth', 'best_move', 'score', 'nodes', 'elapsed_ms']}
                                   for e in record.get('iterations', [])])
            rows.append(row)
    return rows, manifests


def aggregate(rows):
    groups = collections.defaultdict(list)
    for row in rows:
        groups[row['phase'], row['position'], row['variant']].append(row)
    output = []
    for (phase, position, variant), trials in sorted(groups.items()):
        complete = [t for t in trials if t['fixed_depth_complete']]
        times = [t['search_elapsed_ms'] for t in complete if t['search_elapsed_ms'] is not None]
        output.append(dict(phase=phase, position=position, variant=variant, count=len(trials),
                           complete_count=len(complete),
                           moves=dict(collections.Counter(t['best_move'] for t in trials)),
                           depths=dict(collections.Counter(t['completed_depth'] for t in trials)),
                           fixed_complete_median_ms=statistics.median(times) if times else None,
                           fixed_complete_range_ms=[min(times), max(times)] if times else None,
                           fixed_complete_node_counts=sorted({t['nodes_last_iteration'] for t in complete if t['nodes_last_iteration'] is not None})))
    return output


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runs', type=Path, default=HERE / 'runs')
    parser.add_argument('--output', type=Path, default=HERE / 'summary.json')
    args = parser.parse_args()
    rows, manifests = collect(args.runs)
    args.output.write_text(json.dumps(dict(schema_version=1, score_perspective='Black',
        caveats=['Root lines/PV traces can contain bounds; forced-root scores are separate searches.',
                 'Control positions have no correctness oracle; move changes are sensitivity, not strength.',
                 'Completed-iteration nodes exclude unfinished work; final counters include it.',
                 'Incomplete fixed depths are excluded from equal-depth timing summaries.'],
        trial_count=len(rows), runs=rows, groups=aggregate(rows), manifests=manifests), indent=2) + '\n')
    print(f'Summarized {len(rows)} trials into {args.output}')
