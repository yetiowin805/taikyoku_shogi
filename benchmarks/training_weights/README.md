# Cached parent-error weighting audit

Read-only measurement on the frozen v4.5 dataset, using cached 512v3 parent errors.
No new searches, model inference, training, or production writes were performed.

```sh
python3 benchmarks/training_weights/audit.py /path/to/v45/dataset > /tmp/weights-audit.json
```

The report records the samples hash and original parent/model/code bindings.
It covers all 296,817 training rows (247,621 representative, 46,945 precursor,
2,251 mate). Validation/test are excluded from the difficulty analysis.
Median absolute probability error was .03669; p75 .10459; p90 .22472; p95 .29933.
The old steps assigned 219,600 rows 1x, 53,907 rows 1.5x and 23,310 rows 2x.
These errors measure discrepancy with the teacher target, not objective mistakes.

## Chosen curve

`t = min(error / .35, 1)` and `multiplier = 1 + t*t*(3 - 2*t)`.

This is monotone and continuously differentiable at the cap, stays in [1, 2],
and avoids the sudden jumps at errors .10 and .25. It gives approximately:

- .00 error: 1.000x
- .05 error: 1.055x
- .10 error: 1.198x
- .175 error: 1.500x
- .25 error: 1.802x
- .30 error: 1.945x
- .35 or more: 2.000x

The .35 endpoint centers the rise at .175, midway between the previous .10
and .25 thresholds. It is a conservative smoothing choice, not a trained or
playing-strength-optimized parameter.

## Compared after actual group normalization

For each (stratum, related-game group, game, episode/phase) training cell, remove
the old multiplier, apply the candidate curve, then restore the cell's original
total weight. This preserves strata and game/episode balance exactly.

- Smoothstep to .35: 2.123% total-variation distance from old normalized weights.
- Smoothstep to .30: 2.252%.
- Linear ramp to .35: 2.786%.

Total variation is half the summed absolute change in weights, divided by total
training weight: the fraction of weight redistributed, not a loss improvement.
For the chosen curve, the 10th/50th/90th percentiles of new/old effective row
weight are .9387/.9997/1.0843. This is a limited change to the existing recipe.

Training-weight mass in the original error bands shifts from 70.70/20.00/9.30%
to 71.84/19.11/9.05%. Hard examples retain higher raw multipliers; normalization
can reduce a row's absolute effective weight. Rows in uniformly hard cells get
little redistribution, as intended. Mate/precursor/representative totals do not
change. No claim of improved validation loss or playing strength is supported
by this audit.

## Application

The frozen VPS `v45_data.py` preparation script is not tracked on current main.
Rather than changing that immutable copy, the new standard-library utility
`training/nnue/difficulty_weights.py` converts a cached-error dataset into a new
sharded dataset, retaining feature references, labels, parent provenance and
exact validation/test rows. It streams two passes over sample metadata, loads
no feature tensors, and runs no model inference. It replaces an existing boost
instead of compounding it and is weight-idempotent when applied again.

```sh
python3 training/nnue/difficulty_weights.py \
  --source /path/to/frozen/dataset --out /path/to/new-smooth-dataset
```

Use the resulting directory as the trainer's dataset input. An existing output
is refused, hashes/counts are checked, and the completed manifest is written
last. An interrupted output without dataset.json is incomplete; choose a new
output directory on retry. The converter requires cached training errors and
cannot invent them for a newly generated corpus. Future parent scoring should
call `difficulty_weights.multiplier` for the same formula.

No existing dataset or running training process has been switched to this policy.
