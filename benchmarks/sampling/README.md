# Disagreement-weighted coverage sampling

Reproduce from the repository root:

```sh
python3 benchmarks/sampling/disagreement.py /path/to/nnue-v4-vs-v4.5 > /tmp/disagreement.json
```

The checked-in report uses 22 complete downloaded 512v4-versus-v4.5 games:
10 versus 512, 8 versus 384, and 2 each versus 1024 and 1536. It records game
hashes, base revision, exact sampler source hash, per-game distributions and
all ordinary selections. This is an available-game sample, not independent
observations or a strength comparison. Adjacent positions are highly correlated.

## Final policy

For an eligible position, compare the moving player's black-absolute saved score
with the average of the other player's immediately adjacent saved scores. Map
both to sigmoid(score / 2822.9057434005167), and let d be their absolute difference.
These are neighboring boards; the scale is not a calibrated win probability.

Give each position weight `1 + 3 * min(d / 0.60, 1)`, or baseline weight 1 when
adjacent-score information is unavailable. Missing estimates are recorded as
null, not zero. Only finite own-score positions can be ordinary candidates.

1. Exclude mate neighborhoods: 16 plies before through 8 after every recorded
   mate claim, including later claims. Exclude positions less than 10 plies
   from any existing swing/mate target.
2. Accumulate the remaining weights in ply order. Target the midpoints of 12
   equal-mass intervals: `(k + 0.5) / 12`, for k from 0 through 11.
3. Pick the eligible position whose cumulative-mass center is closest to each
   target. Resolve exact ties with a deterministic game-prefix hash. Enforce
   10-ply spacing from all earlier picks; repair conflicts by choosing the
   nearest remaining mass center, which can lie outside the original interval.
   Do not recompute the original mass distribution after each selection.
4. Return fewer than 12 if spacing exhausts eligible candidates. There is no
   global optimization to guarantee a maximum-cardinality spaced set.

All twelve picks follow this one rule; the discarded 8+4 scheme is absent.
Record target and chosen mass fractions, bucket, weight, and disagreement. The
weight describes sampling density, not an inclusion probability or training
loss multiplier. Training weights and targets are unchanged.

The handcrafted-winner sampler uses the same rule for its ordinary samples
(previously 16, now up to 12); existing targeted event quotas remain. Ordinary
changes apply to newly scanned games, preserving prior records. The separate
mate backfill adds missing targets to existing games without discarding labels.
Search cache identities remain unchanged.

Complexity is O(n log n) from ordering, plus at most twelve linear scans. No
engine searches are performed by the sampler or this audit.

## Empirical results

Across 21,729 finite adjacent-score comparisons, disagreement quartiles were
14.4, 29.3 and 47.7 percentage points; the 90th percentile was 60.1. The median
for 512v4 versus 512v4.5 was 22.1 points, versus 384v4.5 40.4 points. The wider
models have only two games each; these numbers are not model-quality rankings.

The final policy selected 12 ordinary positions in every game (264 total).
Their median disagreement was 27.2 points and mean 31.8 points. The global
eligible-position mean after mate exclusion was 31.2 points. Equal per-game
quotas, spacing, fixed targets and deterministic selection mean the pooled
selected distribution need not exceed the pooled baseline at every quantile.

The old greedy draft selected all eight disagreement picks within plies 244–340
of slot 1327. The final method selects three in window 244–343 and spreads the
rest elsewhere. For slot 787, six old greedy picks were in 991–1090; the final
method selects one there. These demonstrate allocation, not improved training
quality. The unit test with a 300-ply phase (250 at d=.40, 50 at d=.45) gives the
slightly higher peak two samples and the surrounding phase ten.
