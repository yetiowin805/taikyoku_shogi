# NNUE speed experiments — 20 September 2026

All three tested ideas improve search throughput in this small local study. The largest standalone code gain is caching a piece’s summed ability vectors. CPU-specific compilation is also effective. Saved undo adds a smaller gain once the cache is enabled. No production settings, networks, checkpoints, or live processes were changed.

## Three-second results

Ratios compare each width against its own generic baseline build, using geometric means of paired nodes-per-second ratios over four positions and two repetitions. These are speedups, not retained fractions of handcrafted speed. Nodes include quiescence and incomplete iterations.

- **CPU-specific compilation:** width 512 **1.45×**; width 2048 **1.88×**.
- **Cached sums of ability vectors:** width 512 **1.50×**; width 2048 **2.05×**.
- **Saved accumulator undo:** width 512 **1.23×**; width 2048 **1.41×**.
- **Cache plus saved undo:** width 512 **1.55×**; width 2048 **2.26×**.
- **All three combined:** width 512 **1.78×**; width 2048 **2.95×**.

Saved undo adds approximately 4% beyond the ability cache at width 512, and 10% at width 2048. It is useful, but much of its standalone benefit overlaps with the cache. The study did not independently test native-plus-cache without saved undo, so the incremental saved-undo benefit under native compilation remains unmeasured.

The combined variant gained in every sampled position. The range was 1.53–2.40× at width 512 and 2.78–3.29× at width 2048. These ranges describe different positions, not statistical confidence intervals. Width 2048 completed depth 3 instead of depth 2 in the opening and middlegame in both repetitions. Its tactical position completed depth 2 once instead of depth 1. Width 512 kept the same completed depths while searching more nodes.

The separate handcrafted native-compilation control gained 3.1%. Combined native NNUE throughput was approximately 88% of native handcrafted throughput at width 512 and 55% at width 2048; generic baselines were 51% and 19% of generic handcrafted throughput. The handcrafted controls ran after the NNUE batch, so these cross-evaluator comparisons are approximate. Learned scores explore different trees; throughput is not a strength ranking.

## Why it helps

The generic output-head microbenchmark took 6.75 µs at width 512 and 27.35 µs at width 2048. Native compilation reduced these to 2.90 and 11.39 µs. Caching ability sums reduces multiple row additions to one vector addition; hot remove/add pairs fell from 3.56 to 0.41 µs at width 512 and 14.29 to 1.56 µs at width 2048. These repeated-key microbenchmarks favor the cache and greatly overstate full-search gains.

The bounded cache added about 8.7 MiB / 33.8 MiB of peak process memory at widths 512 / 2048. Maximum combined-process RSS was 275.9 / 999.7 MiB, approximately 3.3% / 3.5% above baseline. Cache ownership is per accumulator/model, clone starts empty, collisions check full keys, and the sums stay i32. Saved undo reuses allocated buffers rather than allocating on every move.

## Checks and method

- 96 primary searches: widths 512 and 2048; six variants; four positions; two repetitions; three-second limit; depth ceiling 8.
- 16 additional three-second handcrafted searches comparing generic and native builds.
- 48 depth-1 searches across all four positions, plus 24 depth-2 searches on opening/middlegame positions. Within each model/position/depth, scores, full chosen routes, ordered root lines, node counts and quiescence counts match exactly.
- Five targeted NNUE tests passed under all four runtime modes: baseline, cache, saved undo and combined. They cover feature deltas, promotions, clone/model switching, timeout, checkpoint/schema checks, cache collisions and undo.
- All timed searches returned legal moves; maximum primary search elapsed time was 3020.7 ms.
- Intel i7-1255U, one process at a time on CPU 0; shuffled variants per position/model/repetition. No concurrent builds or intentional competing compute.
- Models are the actual admitted v2 checkpoints. Positions and full histories are the same four games used in the preceding speed probe: plies 0, 160, 360 and 2000.
- Search timing excludes loading/replay/initial accumulator construction; it includes search root cloning and filling its initially empty cache. Peak RSS includes the entire process.

## Next-run baseline

Selected for the next tournament run: CPU-specific compilation, cached ability-vector sums, and saved accumulator undo. These are the reference baseline for any further speed experiments. Selection does not mean deployment: PR #107 remains unmerged and the feature remains off by default. Verify on the VPS CPU before rollout.

## Decision

These are promising mechanical candidates requiring no retraining. Prioritize the ability cache and CPU-specific compilation; keep saved undo as a smaller additional improvement. The prototypes remain behind the disabled-by-default `nnue-speed-probes` feature. Before a production rollout, test representative tactical/multi-leg cases more broadly and rebuild/benchmark native compilation on the VPS CPU. Do not copy this laptop-native executable onto the VPS.

The experiment did not test hand-written SIMD, lazy updates, material-only bookkeeping, or smaller network heads. No claim is made about intermediate widths or VPS gains. Targeted checks and a default-build check were run; this was not a full correctness-suite or tournament evaluation.

See README.md for reproduction commands. Raw JSONL, stderr logs, models/game hashes, compiler/build settings, source/binary hashes and randomized orders live under `data/nnue-speed-experiments-20260920/` in the data checkout.
