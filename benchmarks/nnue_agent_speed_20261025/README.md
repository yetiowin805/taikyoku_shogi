# Royal-query search speed: benchmark summary

Baseline: `cefe79608dde9d10bcf4a0afd3a28f2f88a8cdd8`; implementation: `fe792ca4d8ad0ff1884771093d1592dfc1862d44`.

Native release/ThinLTO/codegen-units=1, Rust 1.92.0, i7-1255U pinned to CPU 2. Workload: 15 opening/tactical/endgame/history positions × 3 agents, fixed depths 1–3, eight alternating A/B repetitions (360 pairs). Two persistent processes, randomized case order, one search at a time; loading/replay excluded. Mixed-model requests can reset NNUE caches. No compiler/training/test jobs ran during timing.

| NNUE agent | Pairs | Baseline CPU (ms) | Candidate CPU (ms) | Change | 95% ratio interval |
|---|---:|---:|---:|---:|---:|
| W512_v4.5 | 120 | 182.846 | 174.445 | -4.59% | 0.94877–0.95909 |
| W384_v4.5 | 120 | 184.790 | 176.093 | -4.71% | 0.93786–0.96769 |
| W512_v5c | 120 | 177.623 | 169.496 | -4.58% | 0.94686–0.96292 |
| All | 360 | 181.728 | 173.322 | -4.63% | 0.94804–0.95972 |

CPU columns are equal-case geometric means per search. Ratios below 1 favor the candidate. Intervals use 5,000 paired bootstrap resamples within each fixed case and describe timing uncertainty on this corpus; serial dependence may make them optimistic. They do not establish gains on other workloads/hardware. Separate 90-pair baseline/baseline control: +0.21% CPU (ratio interval 0.99302–1.01119).

All 360 A/B pairs matched move, score, total-node, quiescence-node, static-eval and ordered-root-line signatures; all completed without abort, with NNUE enabled and the full chosen route legal. Each arm visited 32,568,600 nodes, including 24,431,680 quiescence nodes. The v5c best-state re-export matched its stored network hash. Training was not changed.

Observed process-lifetime high-water RSS: 729.54 MiB baseline / 730.57 MiB candidate, including model loading/replay/warmups. Identical-code control processes differed by 7.73 MiB; the A/B difference is not attributed to index overhead. No full-tournament or playing-strength result. Many-royal positions have correctness coverage but no performance measurement.

Validation: formatting and strict Clippy passed; 472 Rust library tests passed (4 ignored), 13 integration tests passed; Ruff passed, 120 deploy / 3 randomized-search / 22 tactical-tree Python tests passed. Rust, Python and NNUE-Python CI passed on the implementation and original results commits.

The versioned workload is `examples/nnue_tournament_bench.rs`; build each revision with `./deploy/build_native.sh --offline --release --example nnue_tournament_bench` in separate target directories. Exact input identities, ordering, binary hashes, raw observations and full distributions are preserved locally under the ignored directory `data/derived/nnue-agent-speed-20261025-review-evidence-9cc5de0/`. Reproduction requires those local/ignored game and model assets. Generated dumps and rejected-experiment results are excluded from this PR's final diff.
