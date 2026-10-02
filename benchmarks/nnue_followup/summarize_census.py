#!/usr/bin/env python3
"""Summarize census JSONL (or .gz) and extract real activations for sparse_micro.rs."""
import gzip
import json
from pathlib import Path
import sys

out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
rows = []
for name in sys.argv[2:]:
    with (gzip.open(name, 'rt') if name.endswith('.gz') else open(name)) as f:
        rows.extend(json.loads(line) for line in f)
summary = {}
for model in sorted({r['agent'] for r in rows}):
    samples = [r['census'] for r in rows if r['agent'] == model]
    totals = {k: sum(s[k] for s in samples) for k in samples[0] if k != 'samples'}
    totals.update(zero_fraction=totals['zeros']/totals['inputs'],
                  zero_block_fraction=totals['zero_blocks']/totals['blocks'],
                  delta_hit_fraction=totals['delta_hits']/totals['delta_eligible'],
                  changes_per_move=totals['changes']/totals['moves'])
    summary[model] = totals
    (out / (model + '.inputs')).write_bytes(bytes(v for s in samples for x in s['samples'] for v in x))
(out / 'census-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary, indent=2))
