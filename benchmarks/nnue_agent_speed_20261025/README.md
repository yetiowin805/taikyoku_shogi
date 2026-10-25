# Indexed royal queries: native NNUE search measurements

Baseline: `cefe79608dde9d10bcf4a0afd3a28f2f88a8cdd8`. Candidate implementation: `fe792ca4d8ad0ff1884771093d1592dfc1862d44`. Subsequent commits add only these results/documentation. Binary hashes, compiler/platform/build flags and input identities are in [environment.json](environment.json) and [confirmation/plan.json](confirmation/plan.json).

The selected optimization reduces repeated royal-query work in shared search. The expanded comparison measures **4.63% less CPU time** (equal-case paired geometric ratio 0.95375), rather than the initial three-position screen's 7.35%. W384 v4.5, W512 v4.5 and the stored best W512 v5c each show about 4.6% less CPU time on this corpus. This is an agent-search result; training was not changed.

## Aggregate results

| Comparison / metric | Pairs | Baseline total (s) | Candidate total (s) | Equal-case ratio | Change | 95% timing ratio interval |
|---|---:|---:|---:|---:|---:|---:|
| AA / CPU | 90 | 50.033 | 50.243 | 1.00212 | +0.21% | 0.99302–1.01119 |
| AA / wall | 90 | 50.043 | 50.253 | 1.00233 | +0.23% | 0.99311–1.01151 |
| AB / CPU | 360 | 189.147 | 179.682 | 0.95375 | -4.63% | 0.94804–0.95972 |
| AB / wall | 360 | 189.219 | 179.729 | 0.95371 | -4.63% | 0.94796–0.95966 |

Ratios below 1 favor the candidate. Changes and intervals use the equal-case geometric mean of paired candidate/baseline ratios. Raw totals sum all observations and weight expensive searches more heavily; they are separate statistics. A/A compares two baseline processes and is reported independently, with no correction subtracted from A/B.

## By model

| Agent | Cases / pairs | Baseline geometric CPU (ms) | Candidate geometric CPU (ms) | CPU change | 95% timing ratio interval |
|---|---:|---:|---:|---:|---:|
| NNUE_W512_v4.5 | 15 / 120 | 182.846 | 174.445 | -4.59% | 0.94877–0.95909 |
| NNUE_W384_v4.5 | 15 / 120 | 184.790 | 176.093 | -4.71% | 0.93786–0.96769 |
| NNUE_W512_v5c_stored_best | 15 / 120 | 177.623 | 169.496 | -4.58% | 0.94686–0.96292 |

The CPU columns are geometric means with equal weight per case, making their ratio equal to the paired aggregate. Every model has 15 cases × 8 pairs. Interval estimates use 5,000 paired percentile bootstrap resamples **within each fixed case**; cases/models are not resampled. They describe timing variation on this corpus and assume exchangeable pairs. Serial dependence, drift and the small repetition count can make intervals optimistic. They do not establish generalization to other workloads or hardware. Seeds and full distributions are in [per-model-statistics.json](per-model-statistics.json) and [detailed-statistics.json](confirmation/detailed-statistics.json).

## Workload, parity and environment

- 15 positions: 12 existing roots spanning opening, tactical, endgame and 2,000-ply history, plus three prefixes (plies 62/200/400) from a recent v4.5-versus-v5c game. Depths 1–3 are fixed per case, identical across models/arms. This overlaps the screening positions; it is an expanded corpus, not an entirely held-out sample.
- 45 cases × 8 A/B repetitions = 360 measured pairs, plus 90 A/A pairs and 45 warmup pairs per stage. All requested depths completed, with no aborts. Every response reports NNUE enabled. Move/score/total-node/quiescence-node/static-eval/ordered-root-line signatures match, and the Rust helper independently verifies legality of the full chosen route.
- Each A/B arm visited 32,568,600 total nodes, including 24,431,680 quiescence nodes. Quiescence counts are a subset of total nodes.
- Intel i7-1255U, Rust 1.92.0 / LLVM 21.1.3, native release build with ThinLTO/codegen-units=1, pinned to CPU 2. One search at a time, two persistent processes, separately warmed A/A and A/B stages, shuffled case order and exactly alternating arm order per case/repetition. No concurrent compiler, training or test jobs during timings.
- Model loading and game replay are outside the search timer. Root preparation/search is inside. Mixed-model requests can reset thread-local NNUE row/delta caches; this measures repeated mixed-corpus searches, not continuously warm single-model operation.
- v5c uses the checkpoint's stored best configuration and network; the re-exported network hash exactly matches its recorded published best blob (`c6dd6ac8a01fbcf017efefb52ae3cdd6d2dc1cf9051ee5b7d081beaffc78d2f0`). No training was run.
- Observed A/B process-lifetime high-water RSS: baseline 729.54 MiB, candidate 730.57 MiB. This includes model/replay/warmup memory and preceding searches, not isolated search allocations. A/A identical-code peaks differed by 7.73 MiB; the A/B difference is not an attributed memory-cost estimate.

## Reproduction and files

`examples/nnue_tournament_bench.rs` is the versioned search workload. Build each recorded source revision separately with:

```sh
./deploy/build_native.sh --offline --release --example nnue_tournament_bench --target-dir /tmp/search-bench-TOPIC
```

Use distinct target directories, or clean the package before switching independently copied source trees; otherwise Cargo can reuse stale artifacts. Preserve each binary and verify its SHA256. Extract `corpus` from `confirmation/plan.json` into a corpus JSON file, then launch each binary as `taskset -c 2 BINARY CORPUS.json`. It accepts JSON-lines requests with `position`, `model` and `depth`, returning one response per request. For each stage, warm both processes using the saved warmup requests, then replay each raw pair in its recorded order, sending only one request at a time. Restart with two baseline binaries for A/A and baseline/candidate for A/B. Preserve the plan's seed/order and do not compile/train/test during timing. The raw JSONL stores complete measurements, exact-signature digests and ordering; input paths/checksums and binary checksums are in the plan. These ignored/local game and model assets are required: a source checkout alone does not contain all benchmark inputs.

For each case, compute the mean of `log(candidate_cpu_ms / baseline_cpu_ms)` across its eight pairs; average these 45 case means and exponentiate. To reproduce intervals, independently resample eight paired observations with replacement within every fixed case, recompute that aggregate 5,000 times, and interpolate the 2.5th/97.5th percentiles. CPU/wall use the same resampled pair indices. See the saved interval seeds/method and raw checksums in `detailed-statistics.json`. Geometric point estimates require no bootstrap.

[validation.json](validation.json) records checks and node counts. [exploratory-screens.json](exploratory-screens.json) records smaller screens of allocation removal, VirtualBoard filtering and NNUE kernels; those are deferred, and their small-screen changes are not confirmed speed claims. The adopted royal screen also applied the gate patch SHA256 `11543a90c6309266f3b8d73581979279d2e757baf51b8526039bc843dd6b8735`.

## Per-case CPU observations

All rows have eight pairs. Ranges span individual timed searches, not request-tail latency estimates or confidence intervals for each case.

| Position | Agent | Depth | Baseline CPU median [min, max] ms | Candidate CPU median [min, max] ms | Paired ratio |
|---|---|---:|---:|---:|---:|
| opening (ply 0) | NNUE_W512_v4.5 | 3 | 94.082 [88.887, 202.502] | 86.219 [82.705, 189.748] | 0.92591 |
| opening (ply 0) | NNUE_W384_v4.5 | 3 | 171.057 [155.934, 295.514] | 159.698 [147.094, 221.395] | 0.90887 |
| opening (ply 0) | NNUE_W512_v5c_stored_best | 3 | 120.902 [107.540, 155.268] | 114.002 [101.267, 171.433] | 0.96994 |
| middlegame (ply 160) | NNUE_W512_v4.5 | 3 | 332.349 [320.221, 713.499] | 313.941 [303.503, 674.809] | 0.94029 |
| middlegame (ply 160) | NNUE_W384_v4.5 | 3 | 335.753 [316.932, 424.581] | 307.824 [296.173, 401.904] | 0.92630 |
| middlegame (ply 160) | NNUE_W512_v5c_stored_best | 3 | 353.399 [340.002, 446.718] | 328.901 [317.639, 399.928] | 0.92995 |
| tactical (ply 360) | NNUE_W512_v4.5 | 1 | 17.655 [16.395, 24.731] | 17.311 [15.603, 21.224] | 0.95952 |
| tactical (ply 360) | NNUE_W384_v4.5 | 1 | 15.816 [13.075, 21.429] | 16.282 [12.764, 20.564] | 0.99715 |
| tactical (ply 360) | NNUE_W512_v5c_stored_best | 1 | 17.284 [13.103, 21.363] | 16.534 [12.697, 20.230] | 0.98115 |
| long-history (ply 2000) | NNUE_W512_v4.5 | 3 | 288.208 [274.842, 361.530] | 277.335 [261.257, 347.160] | 0.95664 |
| long-history (ply 2000) | NNUE_W384_v4.5 | 3 | 246.065 [235.060, 305.519] | 231.423 [227.107, 284.252] | 0.92839 |
| long-history (ply 2000) | NNUE_W512_v5c_stored_best | 3 | 549.937 [521.283, 832.745] | 519.779 [497.681, 724.606] | 0.93470 |
| early-development (ply 21) | NNUE_W512_v4.5 | 2 | 61.373 [58.451, 76.428] | 57.716 [55.728, 71.406] | 0.94719 |
| early-development (ply 21) | NNUE_W384_v4.5 | 2 | 41.693 [37.958, 73.586] | 39.804 [35.587, 71.740] | 0.96065 |
| early-development (ply 21) | NNUE_W512_v5c_stored_best | 2 | 59.013 [54.587, 70.733] | 55.258 [50.323, 68.191] | 0.93207 |
| midgame-independent (ply 187) | NNUE_W512_v4.5 | 2 | 98.274 [88.471, 215.915] | 100.133 [84.892, 208.060] | 0.98236 |
| midgame-independent (ply 187) | NNUE_W384_v4.5 | 2 | 99.184 [92.808, 120.273] | 98.335 [93.259, 117.210] | 0.98552 |
| midgame-independent (ply 187) | NNUE_W512_v5c_stored_best | 2 | 77.406 [68.499, 163.695] | 71.101 [67.961, 158.814] | 0.96447 |
| late-independent (ply 792) | NNUE_W512_v4.5 | 1 | 726.454 [711.900, 912.830] | 675.317 [665.621, 852.538] | 0.93554 |
| late-independent (ply 792) | NNUE_W384_v4.5 | 1 | 682.216 [660.893, 873.329] | 639.741 [617.734, 1195.917] | 0.97743 |
| late-independent (ply 792) | NNUE_W512_v5c_stored_best | 1 | 704.185 [687.181, 895.296] | 653.728 [639.164, 862.747] | 0.94359 |
| late-royal (ply 842) | NNUE_W512_v4.5 | 3 | 204.376 [199.602, 263.294] | 197.522 [191.267, 246.650] | 0.95819 |
| late-royal (ply 842) | NNUE_W384_v4.5 | 3 | 171.464 [165.374, 208.318] | 165.255 [156.634, 212.609] | 0.97093 |
| late-royal (ply 842) | NNUE_W512_v5c_stored_best | 3 | 88.870 [85.286, 190.572] | 86.500 [81.676, 182.934] | 0.96643 |
| champs-midgame (ply 151) | NNUE_W512_v4.5 | 1 | 102.251 [94.463, 118.864] | 95.213 [89.992, 111.218] | 0.93454 |
| champs-midgame (ply 151) | NNUE_W384_v4.5 | 1 | 100.894 [97.798, 193.190] | 95.682 [94.092, 116.475] | 0.90628 |
| champs-midgame (ply 151) | NNUE_W512_v5c_stored_best | 1 | 104.796 [96.405, 245.263] | 99.397 [92.232, 232.947] | 0.95415 |
| twins-midgame-a (ply 162) | NNUE_W512_v4.5 | 2 | 120.981 [106.341, 139.926] | 115.061 [103.384, 142.095] | 0.95993 |
| twins-midgame-a (ply 162) | NNUE_W384_v4.5 | 2 | 119.421 [107.160, 138.965] | 113.572 [103.711, 136.677] | 0.95803 |
| twins-midgame-a (ply 162) | NNUE_W512_v5c_stored_best | 2 | 114.438 [106.715, 150.493] | 108.281 [100.022, 137.820] | 0.94805 |
| twins-midgame-b (ply 128) | NNUE_W512_v4.5 | 1 | 31.791 [28.390, 41.131] | 31.198 [27.368, 41.407] | 0.97889 |
| twins-midgame-b (ply 128) | NNUE_W384_v4.5 | 1 | 29.804 [24.422, 33.771] | 29.587 [23.003, 33.221] | 0.97896 |
| twins-midgame-b (ply 128) | NNUE_W512_v5c_stored_best | 1 | 32.273 [27.093, 45.687] | 30.624 [26.704, 43.608] | 0.96209 |
| late-endgame (ply 1146) | NNUE_W512_v4.5 | 2 | 30.311 [26.132, 36.493] | 30.725 [25.683, 35.857] | 0.99095 |
| late-endgame (ply 1146) | NNUE_W384_v4.5 | 2 | 23.345 [19.067, 30.592] | 22.389 [19.129, 27.320] | 0.95576 |
| late-endgame (ply 1146) | NNUE_W512_v5c_stored_best | 2 | 181.286 [171.744, 214.623] | 175.728 [165.421, 204.040] | 0.96854 |
| v45-v5c-slot1517-p0062 (ply 62) | NNUE_W512_v4.5 | 3 | 722.311 [716.362, 957.293] | 688.869 [669.217, 857.945] | 0.94074 |
| v45-v5c-slot1517-p0062 (ply 62) | NNUE_W384_v4.5 | 3 | 497.634 [486.171, 605.247] | 460.948 [452.833, 567.672] | 0.93422 |
| v45-v5c-slot1517-p0062 (ply 62) | NNUE_W512_v5c_stored_best | 3 | 229.476 [222.591, 498.564] | 214.293 [208.074, 475.077] | 0.96277 |
| v45-v5c-slot1517-p0200 (ply 200) | NNUE_W512_v4.5 | 3 | 1099.405 [1054.839, 1421.067] | 1053.490 [1000.815, 1277.218] | 0.94390 |
| v45-v5c-slot1517-p0200 (ply 200) | NNUE_W384_v4.5 | 3 | 5740.931 [5681.334, 8659.550] | 5562.755 [5476.869, 7564.520] | 0.95166 |
| v45-v5c-slot1517-p0200 (ply 200) | NNUE_W512_v5c_stored_best | 3 | 926.538 [895.522, 1146.473] | 888.903 [838.405, 1034.586] | 0.94630 |
| v45-v5c-slot1517-p0400 (ply 400) | NNUE_W512_v4.5 | 3 | 2908.801 [2707.842, 3585.304] | 2806.244 [2600.099, 3374.481] | 0.95881 |
| v45-v5c-slot1517-p0400 (ply 400) | NNUE_W384_v4.5 | 3 | 1956.650 [1847.159, 2486.313] | 1827.396 [1777.941, 2368.201] | 0.95944 |
| v45-v5c-slot1517-p0400 (ply 400) | NNUE_W512_v5c_stored_best | 3 | 1497.152 [1458.563, 1922.067] | 1445.788 [1399.640, 1696.228] | 0.95118 |

## Limits and implementation tradeoffs

These are local fixed-depth observations, with no time-control playing-strength, full-tournament wall-time, trainer-throughput, multi-thread scaling, cross-architecture or server-hardware result. Faster time-limited search can select a different move by exploring more work. Board storage adds two vector headers (48 bytes on this host) plus small royal-index allocations and maintenance. Correctness tests include edited many-royal positions; performance of such positions was not measured. Ordered capture generation, model weights, formats and search defaults are unchanged.
