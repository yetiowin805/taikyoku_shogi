# One-pass feature counting and efficiency priorities

Baseline source: `c73a7d5ecdd1f179213c1f5664b39a92178ae26b`. Candidate source: `40593764a6d0d7ecb9f96c174c0e56b69f385652`.
The PR base `87762c0` adds a survey document and has identical Rust source to the baseline.

## Method and correctness

The optimized temporary helper calls `training::featurize::featurize_game` on the
12 original search-speed corpus games with `EvalWeights::seed()` and default
sampling (up to 150 rows/game). Each process loads the record, warms extraction,
and fingerprints every output row before timing one complete extraction. Input
loading, row serialization, and output-file writes are outside timing.

Six alternating-order pairs per game, shuffled case order per repeat, give
**72 pairs over 12 games, 10,931 moves and 1,257 exported rows**.
Full serialized row signatures matched in all 72 pairs, including feature order,
labels, turn, ply and seed evaluation. Unit tests cover every piece type plus
promotion/capture changes. Source-derived temporary army-vector clones fall from
608 to zero per position; this is not a global allocation-profiler measurement.

Builds use rustc 1.92.0 and the optimized release library; the helper uses `-O -C
lto=thin`. Both processes are pinned to CPU 2 on 12th Gen Intel(R) Core(TM) i7-1255U.
The host is shared; frequency and unrelated load are not controlled. No compiler
or test run overlaps recorded timings. Results and input/binary/helper hashes are
in `results.json`, with every raw pair in `pairs.jsonl`.

## Timing results

Times below are milliseconds per complete in-memory game extraction. Reduction
and ratio use the geometric mean of within-pair candidate/baseline ratios; the
baseline/candidate columns separately show medians, so their quotient can differ.
Intervals are percentile bootstraps of six paired log ratios (10,000 resamples).

| Workload | Pairs | Baseline median (ms) | Candidate median (ms) | Time reduction | Paired ratio [95% CI] |
|---|---:|---:|---:|---:|---:|
| opening (671 moves / 136 rows) | 6 | 82.866 | 10.789 | 87.1% | 0.129 [0.127, 0.131] |
| middlegame (400 moves / 96 rows) | 6 | 58.694 | 7.398 | 87.3% | 0.127 [0.125, 0.129] |
| tactical (434 moves / 92 rows) | 6 | 66.151 | 9.097 | 86.2% | 0.138 [0.133, 0.143] |
| long-history (4419 moves / 150 rows) | 6 | 526.494 | 64.560 | 87.0% | 0.130 [0.120, 0.147] |
| early-development (547 moves / 107 rows) | 6 | 79.478 | 10.529 | 86.5% | 0.135 [0.131, 0.139] |
| midgame-independent (341 moves / 76 rows) | 6 | 54.856 | 7.695 | 85.9% | 0.141 [0.137, 0.144] |
| late-independent (800 moves / 105 rows) | 6 | 133.489 | 17.779 | 85.7% | 0.143 [0.130, 0.166] |
| late-royal (1124 moves / 118 rows) | 6 | 120.878 | 16.443 | 86.4% | 0.136 [0.134, 0.140] |
| champs-midgame (167 moves / 40 rows) | 6 | 31.757 | 4.617 | 85.0% | 0.150 [0.140, 0.166] |
| twins-midgame-a (264 moves / 55 rows) | 6 | 43.103 | 6.051 | 86.1% | 0.139 [0.134, 0.143] |
| twins-midgame-b (574 moves / 132 rows) | 6 | 80.778 | 10.491 | 86.7% | 0.133 [0.129, 0.137] |
| late-endgame (1190 moves / 150 rows) | 6 | 133.485 | 17.386 | 87.0% | 0.130 [0.128, 0.132] |

The equal-game aggregate time ratio is **0.1358**
(**86.4% less time; 7.36x throughput**).
A game-cluster bootstrap retaining all six pairs per game gives a local 95%
interval of [0.1325,
0.1396]. The median 12-game batch
fell from 1419.7 to 189.8 ms.

## Observed variability

These are observed ranges across six measurements, not per-request tail latency
estimates. `results.json` additionally records quartiles, means and sample standard
deviations.

| Workload | Baseline observed range (ms) | Candidate observed range (ms) |
|---|---:|---:|
| opening | 77.451–91.234 | 9.825–11.457 |
| middlegame | 57.928–60.287 | 7.281–7.804 |
| tactical | 62.858–66.488 | 8.499–9.461 |
| long-history | 509.612–540.042 | 62.308–90.839 |
| early-development | 72.926–85.333 | 9.624–11.904 |
| midgame-independent | 52.951–57.694 | 7.362–8.512 |
| late-independent | 130.437–138.516 | 16.849–28.446 |
| late-royal | 119.646–131.175 | 15.986–17.906 |
| champs-midgame | 30.403–32.787 | 4.148–6.204 |
| twins-midgame-a | 42.354–45.825 | 5.879–6.222 |
| twins-midgame-b | 74.868–82.058 | 9.833–11.668 |
| late-endgame | 129.538–136.341 | 16.969–17.515 |

## Process memory

Median peak RSS includes record loading and the warmup, which are outside timing.
This independent branch retains the main-branch JSON loader; its intermediate tree
influences process peaks. The measurements do not show a peak-memory improvement.
Per-ply snapshots/evaluations are still retained before sampling.

| Workload | Baseline peak RSS (MiB) | Candidate peak RSS (MiB) | Difference (MiB) |
|---|---:|---:|---:|
| opening | 5.00 | 5.12 | +0.12 |
| middlegame | 4.25 | 4.25 | +0.00 |
| tactical | 5.81 | 5.88 | +0.06 |
| long-history | 16.62 | 16.62 | +0.00 |
| early-development | 5.31 | 5.38 | +0.06 |
| midgame-independent | 5.12 | 5.06 | -0.06 |
| late-independent | 4.69 | 4.75 | +0.06 |
| late-royal | 7.75 | 7.88 | +0.12 |
| champs-midgame | 4.75 | 4.75 | +0.00 |
| twins-midgame-a | 4.88 | 5.00 | +0.12 |
| twins-midgame-b | 5.38 | 5.50 | +0.12 |
| late-endgame | 6.62 | 6.62 | +0.00 |

## How this compares with the other findings

| Finding | Evidence | Practical next step |
|---|---|---|
| F23 repeated army copying | Long game: 526.5 to 64.6 ms; exact row parity | Retain the measured one-pass implementation in this PR. |
| Strict-loader regression | Separate PR #188: long v2 parse 11.5 to 2.2 ms | Restore direct decoding; combine with this independent preparation improvement. |
| F31 historical freeze cache | Static inspection: all 15 manifest engines enter worktree/build paths on every freeze | Highest apparent warm-restart opportunity: verify cache identities and reuse binaries, with invalidation/recovery tests. No historical build was run for this screen. |
| F22 repeated fits / growing checkpoints | Synthetic checkpoint results below; save calls perform two rating fits in order-neutral mode | Reuse one fit first; profile the live run before redesigning durable state/report persistence. |
| F16 repetition-history scans | Linear full-history queries remain in AB and quiescence | Measure an exact index/prefix-count experiment with short/long histories and real NNUE models before adopting it. |
| F38 snapshot clones | Four avoidable army clones per export by source inspection | Small cleanup; lower priority than work repeated at every ply or scheduler commit. |

The exploratory F22 screen used 40 synthetic entrants and two repetitions per
size on the local filesystem. It measures no live worker lock-wait/utilization:

| Retained slots | One rating fit (ms) | Full checkpoint (ms) | Bytes written per checkpoint |
|---|---:|---:|---:|
| 1,000 | 2.36 | 14.30 | 337,035 |
| 10,000 | 10.98 | 39.12 | 3,190,150 |
| 100,000 | 103.65 | 324.19 | 32,080,191 |

## Limits

This is the Texel/handcrafted feature-extraction path, not the NNUE tensor-export
pipeline, whole training cycle, or per-move engine search. No weights, sampling,
evaluation formulas, search policy, acceptance rules or dataset publication
protocol change. This corpus is reused rather than held out. Local bootstrap
intervals describe these measurements and cases; they do not establish performance
on other hardware, unseen games, disk-bound exports, or playing strength.
