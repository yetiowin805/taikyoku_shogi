#!/usr/bin/env python3
"""Summarize the five-position A/L smoke study (not a strength estimate)."""
import json
import math
from pathlib import Path
import statistics
import sys


def summarize(directory):
    directory = Path(directory)
    cases = [c['name'] for c in json.loads((directory/'corpus.json').read_text())]
    grid = [json.loads(line) for line in (directory/'grid-smoke.jsonl').read_text().splitlines()]
    rows = [json.loads(line) for line in (directory/'search-new.jsonl').read_text().splitlines()]
    lookup = {(r['case'], r['rep'], r['variant']): r for r in rows}
    result = {'grid_searches': len(grid), 'grid_agents': len({r['agent'] for r in grid}),
              'grid_max_ms': max(r['ms'] for r in grid), 'timed_searches': len(rows), 'variants': {}}
    for variant in ('Ldefense', 'Lmate'):
        pairs = [(lookup[(c, rep, 'off')], lookup[(c, rep, variant)]) for c in cases for rep in range(2)]
        if any(min(a['ms'], b['ms']) < 2900 for a, b in pairs):
            raise ValueError('a corpus search ended early; report separately from budget-limited searches')
        ratios = [(b['nodes']/b['ms'])/(a['nodes']/a['ms']) for a, b in pairs]
        per = {c: statistics.mean(ratios[i] for i, (a, _) in enumerate(pairs) if a['case'] == c) for c in cases}
        result['variants'][variant] = {
            'nps_percent': 100*(math.exp(statistics.mean(math.log(x) for x in per.values()))-1),
            'changed_moves': sum(a['best'] != b['best'] for a, b in pairs),
            'depth_delta_mean': statistics.mean(b['depth']-a['depth'] for a, b in pairs),
            'extensions': sum(b['royal_extensions'] for _, b in pairs),
            'probe_max_us': max((b.get('royal_probe') or {}).get('elapsed_us', 0) for _, b in pairs),
            'probe_found': sum((b.get('royal_probe') or {}).get('status') == 'Found' for _, b in pairs),
            'max_ms': max(b['ms'] for _, b in pairs),
            'per_position_nps_percent': {k: 100*(v-1) for k, v in per.items()},
        }
    return result


if __name__ == '__main__':
    print(json.dumps(summarize(sys.argv[1]), indent=2))
