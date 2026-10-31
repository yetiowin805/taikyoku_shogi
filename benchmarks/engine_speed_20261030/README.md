# Engine speed sweep — October 30–31, 2026

The retained native bundle uses **3.13% less CPU** on nine reserved roots from seven games, across seven agents (378 exact pairs). Compiler PGO changes CPU by **-11.61% versus that selected native build** on the same held-out roots (252 pairs). These comparisons are measured separately; their percentages are not multiplied.

Baseline `caf49f64a5a6eb2a1ba2660040676217bc1ce794`; retained source `480393c630d4fac476ec0df53d9e19bb4c78c9cc`. The sweep screened **22 source variants**, revisited previous results and read primary engine sources/papers. Across all archived searches, **2,520 measured pairs and 938 warmup pairs** match the complete search signature. No playing-strength, cold-start or whole-tournament gain is claimed.

A separate **direct final PGO versus original baseline** comparison uses 14.61% less CPU (126 pairs; game-cluster ratio interval 0.84667–0.86003). This is an observed total effect, not a multiplication.

## Definitely good

Keep the confirmed exact bundle and the nondefault TT capacity fix. Keep the reusable paired benchmark runner. PGO remains an opt-in local build with its own measured comparison; no tournament or production binary was started/replaced. Individual retained screens are leads within the confirmed package, not isolated causal estimates of every member.

**Existing opt-in PGO workflow: Definitely good on this measured host/profile.** Its separate held-out and sequence comparisons are below. This is a compiler-build result, not a new default or a training/weight change. Recheck another CPU/toolchain/profile before generalizing.

## Measured comparisons

CPU columns are equal-case geometric means per search in milliseconds. Ratio intervals resample complete source games. Roots/games counts distinguish related roots. Lower is better.

| Comparison | Pairs | Roots / games | Baseline CPU (ms) | Candidate CPU (ms) | CPU change | 95% game-cluster ratio interval |
|---|---:|---:|---:|---:|---:|---|
| Selected native / original native | 378 | 9 / 7 | 188.975 | 183.061 | -3.13% | 0.96272–0.97606 |
| Selected native / original, new consecutive roots | 448 | 16 / 2 | 100.188 | 96.122 | -4.06% | 0.95724–0.96161 |
| Selected PGO / selected native | 252 | 9 / 7 | 185.978 | 164.387 | -11.61% | 0.87524–0.89228 |
| Selected PGO / selected native, new consecutive roots | 224 | 16 / 2 | 98.861 | 88.808 | -10.17% | 0.89133–0.90536 |
| Selected PGO / original native (direct total) | 126 | 9 / 7 | 187.046 | 159.713 | -14.61% | 0.84667–0.86003 |

The two-game sequence workload is additional evidence with limited generality, even though it contains 448 pairs. Summed CPU ratios (candidate/baseline), a different weighting from the primary geometric mean, are 0.96820 native-confirmation, 0.95026 native-sequence, 0.88056 PGO-confirmation and 0.89833 PGO-sequence. Wall-time ratios and whole-process peak RSS are preserved in [aggregate.json](aggregate.json).

### Native bundle by agent

| Agent | Pairs | Baseline CPU (ms) | Candidate CPU (ms) | Change | 95% game-cluster ratio interval |
|---|---:|---:|---:|---:|---|
| BASE_C2S2_A40_Lmate | 54 | 279.397 | 267.044 | -4.42% | 0.94174–0.97373 |
| C2K50A1 | 54 | 220.769 | 211.204 | -4.33% | 0.94757–0.96939 |
| NNUE_W1536_v3 | 54 | 138.932 | 132.876 | -4.36% | 0.93124–0.97422 |
| NNUE_W2048_v2 | 54 | 180.830 | 174.284 | -3.62% | 0.95029–0.97276 |
| NNUE_W384_v4.5 | 54 | 217.401 | 213.537 | -1.78% | 0.97452–0.99010 |
| NNUE_W512_v3 | 54 | 124.048 | 121.584 | -1.99% | 0.96042–1.01413 |
| NNUE_W512_v4.5 | 54 | 205.943 | 203.158 | -1.35% | 0.97385–1.00658 |

### PGO versus selected native by agent

| Agent | Pairs | Baseline CPU (ms) | Candidate CPU (ms) | Change | 95% game-cluster ratio interval |
|---|---:|---:|---:|---:|---|
| BASE_C2S2_A40_Lmate | 36 | 274.457 | 239.199 | -12.85% | 0.85512–0.88380 |
| C2K50A1 | 36 | 213.816 | 181.871 | -14.94% | 0.83256–0.86754 |
| NNUE_W1536_v3 | 36 | 132.770 | 121.505 | -8.48% | 0.89758–0.92562 |
| NNUE_W2048_v2 | 36 | 177.490 | 162.018 | -8.72% | 0.90326–0.92278 |
| NNUE_W384_v4.5 | 36 | 207.378 | 182.808 | -11.85% | 0.87398–0.89278 |
| NNUE_W512_v3 | 36 | 130.594 | 113.421 | -13.15% | 0.85028–0.89762 |
| NNUE_W512_v4.5 | 36 | 205.473 | 182.689 | -11.09% | 0.88163–0.89466 |

### Final PGO versus original native (direct measurement)

| Agent | Pairs | Baseline CPU (ms) | Candidate CPU (ms) | Change | 95% game-cluster ratio interval |
|---|---:|---:|---:|---:|---|
| BASE_C2S2_A40_Lmate | 18 | 292.915 | 240.439 | -17.92% | 0.80794–0.83524 |
| C2K50A1 | 18 | 222.459 | 177.836 | -20.06% | 0.77197–0.82078 |
| NNUE_W1536_v3 | 18 | 134.133 | 120.058 | -10.49% | 0.86544–0.91751 |
| NNUE_W2048_v2 | 18 | 175.939 | 159.975 | -9.07% | 0.88900–0.92624 |
| NNUE_W384_v4.5 | 18 | 213.496 | 182.864 | -14.35% | 0.84939–0.86926 |
| NNUE_W512_v3 | 18 | 120.826 | 102.379 | -15.27% | 0.83583–0.86157 |
| NNUE_W512_v4.5 | 18 | 201.926 | 172.416 | -14.61% | 0.84333–0.86149 |

### Controls

| A/A control | Pairs | Roots / games | First arm CPU (ms) | Second arm CPU (ms) | CPU change | 95% game-cluster ratio interval |
|---|---:|---:|---:|---:|---:|---|
| Original six-root baseline control | 84 | 6 / 6 | 99.975 | 100.143 | +0.17% | 0.99320–1.00963 |
| Reserved-root baseline control | 126 | 9 / 7 | 192.435 | 193.961 | +0.79% | 0.99953–1.02188 |

## Retained implementation

The accepted **bundle** is the unit of final performance confirmation. Individual screens select its members; their percentages are not added or multiplied, and inclusion in a winning bundle does not isolate each member's final contribution.

- `GameState::prepare_search_repetition` builds an immutable sorted count index for histories of at least 128 entries. A 2 KiB absence bitmap can reject absent keys before binary search; matching bits still require exact 64-bit identity. The short appended search suffix stays in the existing Vec. Capture/promotion progress resets do not discard history. Reset clears the prefix, pop/undo invalidate it if they cross its boundary, and clones share only the immutable prefix. On this 64-bit host the count-vector capacity adds about 16 bytes per history entry plus the bitmap, with a transient 8-byte-per-entry key copy during construction; those are source-derived payload estimates, not RSS measurements. Root construction is inside the search timer.
- `MovementGenerator` appends ordinary Simple, Range and Jumping targets directly into the final growable target buffer. Compound/conditional/Free Eagle fallbacks retain their existing intermediate lists. A shared Range visitor can count raw first-leg landings without allocating a Vec, preserving blocking, prohibited-jump, sweep and duplicate semantics. The final target sort/dedup and move-route order remain unchanged. Free Eagle's single-capability call borrows a one-element slice rather than cloning a capability into a Vec.
- `direction_iter` walks set bits with `trailing_zeros` and clear-lowest-bit, preserving N, NE, E, SE, S, SW, W, NW order for every one of the 256 masks. Existing board bitboards already used this technique; the new change applies it to direction sets.
- `Accumulator::save_and_apply` consumes a cloneable iterator over borrowed subtraction pieces, avoiding the temporary mover-plus-captures Vec on every neural move. Existing whole-move delta keys, cached fused rows, collision checks and wide-integer overflow fallback remain in place.
- The existing four-output AVX-VNNI first-dense-layer block reduces all four output vectors together. Two horizontal-add stages and a half-vector addition produce the same row sums without four independent lane reductions. Weight layout, checkpoint format, architecture, output normalization and model values remain unchanged. The generic and other SIMD fallbacks are unchanged. This specific arithmetic gain was tested on an AVX2/AVX-VNNI host.

### A separate TT capacity bug

The table now stores its set mask when constructed. It also rounds the number of sets down to the largest fitting power of two, independently of cluster width. Default width 1 and widths 2/4/8 retain the old capacity and key mapping. Widths 3/5/6/7 previously masked with a non-power-of-two set count, leaving many allocated sets unreachable. The fix can change search results under those nondefault Rust configurations by restoring useful table capacity; this is a bug fix, not a production/default speed claim.

For a requested budget of 1,048,576 entries and width 3, the old code allocated 1,048,575 entries in 349,525 nominal sets but could address only 512 sets (1,536 entries). The new code allocates 786,432 entries in 262,144 fully addressable sets. These counts are derived directly from the indexing formulas, not a measured hit-rate or playing-strength result. Tests cover all widths, requested sizes around a power-of-two boundary, every set's reachability, largest-fitting capacity and full-key population of every slot.

No search-policy toggle, model weight, network format, pruning/eligibility rule, reduction schedule, ordering tie policy or `models/history` logic snapshot was added. The nondefault TT capacity behavior is the material exception described above. All rejected experiment paths are absent from the retained production diff.


## Promising

1. **Clock and partial-iteration work, tested with realistic clocks.** Newer existing logs attribute 35.86% of summed recorded iteration time to incomplete iterations over 12,022 corrected-build HLF/HLFS nonmate turns. This is not recoverable-speed evidence. Existing GrowthAware behavior was inactive in a fixed-time screen, so compare realistic clocks and decision quality before adding another heuristic.
2. **Relative history and cost-aware ordering.** Existing bounded History has actual completed-game evidence. Relative success/attempt normalization, route-aware identities and expected child-cost/cut-probability ordering remain separate untested proposals. They change the searched tree, interact with LMR and require tactical and playing comparisons.
3. **Virtual-board scans for many-removal overlays.** The general rewrite lost in full search and quiet synthetic queries, but large-removal synthetic overlays improved. A reused removal index or a carefully measured hybrid might help actual sweep-heavy API users. Census legal overlays/call counts first.
4. **Stable lazy move selection.** Other engines avoid sorting tails never reached after a cutoff. An exact adaptation must freeze scores before recursion and reproduce original-index ties and TT-swap behavior. No such implementation or gain is claimed here.
5. **Attack-index census before another layout.** The tested distant-direction index was exact but slower for all screened agents. A future capture-path or short-class index must first count rejections after memo hits and before early success, and compare them with unavoidable mutation/clone costs. The tested index did not narrow capture generation.

See the [research and previous-results appendix](research/README.md) for pinned primary sources, applicability, newer agent evidence and rejected transplants.

## All screened implementations

All candidates start from the baseline revision plus their named patch set. [experiments.json](experiments.json) records source-file/patch/binary hashes and build metadata; [plans](plans) record exact workloads. Rejected prototype patches/binaries remain in the ignored local experiment directory, not the production diff or benchmark source copies. Prototype source identity is baseline plus recorded patch hashes; retained source has the Git revision above.

| Experiment | Decision | Pairs | Baseline / candidate CPU (ms) | CPU change | 95% game-cluster ratio interval | Reason |
|---|---|---:|---:|---:|---|---|
| NNUE material-only bookkeeping + small cleanups | Rejected | 126 | 101.78 / 101.47 | -0.30% | 0.99226–1.00160 | No clear aggregate gain; includes unexercised flight early exit and a small borrowing cleanup. |
| NNUE updates from borrowed iterators | Definitely good (bundle) | 36 | 114.58 / 112.62 | -1.70% | 0.96195–1.00540 | Kept in the confirmed bundle; also borrows the Free Eagle capability slice. |
| Immutable repetition-history prefix index | Definitely good (bundle) | 36 | 114.84 / 110.69 | -3.61% | 0.94459–0.98356 | Clearest history-cost lead; full-key prefix counts plus short suffix. |
| Byte-dot second neural layer | Rejected | 36 | 106.73 / 107.82 | +1.02% | 0.99566–1.02512 | Byte-dot rewrite did not beat the existing auto-vectorized i32 layer. |
| Four search cost changes together | Rejected | 36 | 107.91 / 108.56 | +0.60% | 0.97847–1.03380 | Four changes together were mixed; individual ablations below. |
| Consolidated attack-filter metadata | Rejected | 36 | 107.80 / 108.72 | +0.85% | 0.99025–1.02401 | Consolidated metadata/geometry did not establish a win. |
| Allocation-free range landing counts | Definitely good (bundle) | 36 | 127.83 / 127.52 | -0.24% | 0.98045–1.01372 | Standalone screen neutral overall; count visitor retained within direct-append package. |
| Direct target append + range counts | Definitely good (bundle) | 36 | 134.93 / 132.66 | -1.68% | 0.96043–1.00571 | Kept with Range counts; common capabilities share the final target Vec. |
| Borrow occupied square directly | Rejected | 36 | 129.86 / 128.87 | -0.76% | 0.97277–1.01070 | Small uncertain gain did not justify an additional change. |
| Walk only set direction bits | Definitely good (bundle) | 36 | 117.98 / 115.89 | -1.77% | 0.97364–0.99095 | Kept; exact N-through-NW order for every 8-bit mask. |
| Iterator updates + specialized common row shapes | Rejected | 36 | 118.40 / 118.90 | +0.42% | 0.99658–1.01423 | Includes iterator updates; extra two/three-row specialization not retained. |
| Exact shifts after nonnegative clamp | Rejected | 36 | 121.90 / 121.94 | +0.03% | 0.98753–1.01320 | Exact quantization simplification, but no useful overall gain. |
| Smaller tagged landing-attack cache | Rejected | 36 | 136.33 / 136.39 | +0.05% | 0.98416–1.01868 | Smaller tagged cache was mixed; collision fallback can cost work. |
| Borrowed VirtualBoard attack scan | Promising API niche; excluded | 36 | 123.36 / 125.22 | +1.51% | 0.98089–1.06270 | Full search slower; synthetic many-removal API niche remains promising. |
| Iterator updates + fused row fit/write | Rejected | 36 | 118.90 / 117.89 | -0.85% | 0.97424–1.01167 | Includes iterator updates; mixed, not a convincing reason for extra cache-write complexity. |
| Exact output activation lookup | Rejected | 36 | 119.97 / 122.39 | +2.02% | 0.99027–1.05058 | Exact 512-byte activation table was slower in the screen. |
| Four VNNI output reductions together | Definitely good (bundle) | 36 | 119.59 / 117.58 | -1.68% | 0.96928–0.99887 | Kept; four existing VNNI row reductions combined, no weight-layout change. |
| Indexed stable root ordering | Rejected | 36 | 122.83 / 122.44 | -0.32% | 0.97138–1.02092 | Indexed stable root-score lookup was effectively neutral. |
| Stable quiescence candidate retention | Rejected | 36 | 120.28 / 119.69 | -0.49% | 0.98435–1.00640 | Mixed: handcrafted improved, both screened NNUE agents regressed slightly. |
| Cached capture-ordering scores | Rejected | 36 | 116.61 / 117.61 | +0.86% | 0.99734–1.02056 | Extra cached keys/scores did not improve the complete search. |
| Precomputed transposition set mask | Definitely good (TT fix support) | 36 | 114.65 / 114.43 | -0.19% | 0.98181–1.02060 | Timing neutral; retained in support of the separately tested odd-width TT capacity fix. |
| Maintained distant range-direction index | Rejected | 36 | 114.22 / 120.68 | +5.66% | 0.99620–1.11131 | Exact prototype was slower for all three agents after maintenance/clone/query costs. |

Common screen: W512_v4.5, W2048_v2 and handcrafted BASE; six roots from six distinct games; two arm-balanced repetitions (36 pairs). The initial material-only package used all seven agents and three repetitions (126 pairs; approximately balanced order). Fixed-rows and fused-fit include iterator changes; direct-append includes Range counting; iterator includes the Free Eagle slice cleanup; material-only also includes that cleanup and a flight early exit that none of these seven agents exercises (`lr_flight_k == 0`). The four-change search package includes root lookup, q retention, cached capture scores and TT mask. Do not subtract package ratios to infer individual effects.

### Rejected does not mean universally slower

The label means no convincing reason to retain that implementation for this baseline and workload. Most changes are neutral or model-dependent; a few are clearly slower in this screen. Removing a Vec, replacing a division or shrinking a cache can add work elsewhere or change code generation. Screen intervals do not adjust for selection across many candidates. The held-out bundle is a separate confirmation.

The 3,024-byte-per-board direction index preserved near/global/uncertain candidates and accepted-piece order, but CPU increased for every screened agent. Its focused tests passed; correctness was not the reason for rejection. Geometry initialization and some replay costs are outside the warm search timer. The measured loss includes ongoing board maintenance, cloning and query assembly, but does not isolate their individual causal costs.

### Virtual-board subsystem result

Seven alternating trials per fixture, 4,096 queries per arm/trial, pinned CPU 2; same executable/ThinLTO, Instant wall timer. Mean microseconds/query and geometric paired ratios are subsystem evidence, not full-search gains.

| Synthetic fixture | Legacy µs/query | Streamed µs/query | Paired ratio | Trial-ratio range |
|---|---:|---:|---:|---|
| opening / quiet | 3.234 | 3.831 | 1.1842 | 1.1416–1.2296 |
| opening / sweep12 | 4.268 | 3.673 | 0.8605 | 0.8137–0.8938 |
| sparse / quiet | 0.627 | 0.701 | 1.1191 | 1.0655–1.1717 |
| sparse / sweep12 | 0.973 | 0.745 | 0.7650 | 0.6820–0.8590 |

The sparse board has artificial file stripes; overlays teleport a pawn and optionally remove twelve arbitrarily chosen pieces. They are not legal game sequences. Legacy iteration borrows elements of a materialized Vec; the streamed dispatcher is compiled in the production library, so code-generation boundaries differ. Every query answer and hit count matched. Raw rows and the build/fixture plan are archived; this niche remains unshipped.

## Scope and method

This sweep revisited the repository's September–October movement, search and NNUE reports and read primary engine sources and papers. It tested independent implementations and small explicitly named packages, then reserved separate roots for confirmation. The aim was cheaper execution of the same engine policy. No network weights, training dataset, pruning policy or evaluation formula were tuned, and no Elo gain was measured.

Baseline: `caf49f64a5a6eb2a1ba2660040676217bc1ce794`. Every variant was rebuilt from an archive of that revision plus its recorded patch set, using the same native build command, rather than accumulating unrelated experiments. The retained diff removes rejected code. Binary SHA-256, patch SHA-256, source-file SHA-256, build times and compiler/workload metadata are recorded. Rejected source variants and local model/binary copies remain outside the benchmark records.

The existing `nnue_tournament_bench` JSONL executable times the `search(...)` call, including its internal setup, execution and cleanup. Checkpoint/player creation and per-request configuration cloning are outside the timer. Model loading, recorded-game replay and the legal-move cross-check happen outside its timer. `deploy/benchmark_search.py` keeps one model loaded in each of two persistent processes, warms every case, pins both workers to CPU 2, shuffles independent cases and alternates baseline/candidate order. Even repetition counts balance order for every case. Primary metric is process CPU milliseconds (user + system, `getrusage(RUSAGE_SELF)`); wall milliseconds are secondary. The host is an Intel Core i7-1255U laptop, Rust 1.92.0/LLVM 21.1.3, ThinLTO, one codegen unit, `target-cpu=native`. Hardware performance counters were unavailable (`perf_event_paranoid=4`). Engine builds and test suites did not overlap recorded timing. Lightweight metadata/report preparation continued, and the host was not isolated from unrelated activity.

Parity compares the complete selected route, score, total nodes, qnodes, completed depth, static evaluation and ordered root lines, including warmups. The helper validates the selected full route against legal moves. Incomplete searches are rejected. Total nodes already include qnodes; they must not be added together. This checks the exercised workloads, not all possible game positions.

The aggregate is the geometric mean of paired candidate/baseline CPU ratios, with each measured model/root/repetition case weighted equally. A separate ratio of summed CPU time is also recorded. The primary 95% interval resamples complete source games (3,000 deterministic cluster-bootstrap draws), keeping related roots together. Position-cluster intervals are a sensitivity calculation. Few game clusters limit inference, and intervals for screened winners do not account for selection across many candidates. Confirmation is evaluated separately. These are search batch aggregates, not individual-request p95/p99 latency or Elo statistics. RSS is the process high-water mark including model loading and replay, not incremental search allocation.

Screen roots were preselected from existing records: opening, tactical, long-history, two late positions and an endgame, six distinct source games. The common screen uses three agents (W512, W2048 and handcrafted BASE), six roots and two repetitions: 36 exact pairs per variant. The material-only initial package used all seven agents and three repetitions. Nine other roots from seven source games were reserved for confirmation; three of those roots belong to one game and therefore stay in one bootstrap cluster. A second reserved workload uses two source games unused in earlier stages of this sweep, eight consecutive roots per game, depth two and ordered roots, to probe cache behavior beyond shuffled snapshots. It is not a live self-play match or a realistic time-control strength evaluation.

Seven agents span five real NNUE checkpoints (W384, two W512 generations, W1536, W2048) and two handcrafted evaluators. Missing old temporary v5c model files were excluded rather than replaced. Game, checkpoint and referenced network-blob hashes are checked before execution. CPU-time and wall-time raw pairs, warmups, summaries and plans accompany this report. All measured builds use identical weights/configurations for each baseline/candidate pair.


## Previous results and primary sources

The baseline already includes native SIMD inference, fused-row/move-delta caches, parked storage, root reuse, attack/capture/gate memos and royal indexing. None of their old percentages is counted again. Sparse hidden-input kernels, deferred updates, huge pages, transposed heads, fat LTO and line occupancy had neutral/mixed earlier evidence; this sweep revisited concrete arithmetic and allocation variants instead of repeating those recommendations.

The prior-results audit found actual game evidence for bounded History plus newer generation-three matched-depth screens. QHistory was neutral at the median; PVS had case-specific node savings; changed MajorThreatLMR decisions were more expensive at the median. These are not new NPS or strength measurements. Saved source/engine identities, sample sizes, rating-point/selection limitations and the incomplete-iteration analysis are in [search-prior-evidence.json](search-prior-evidence.json) and the appendix.

Primary sources include the original NNUE paper, Stockfish/nnue-pytorch, YaneuraOu, Ethereal, Fairy-Stockfish, relative-history research and fastest-cut-first research. The [source catalog](primary-sources.json) pins engine revisions and file hashes. The appendix links each source beside the claim it supports and explains what was actually transferred or rejected.

## Validation

502 Rust tests passed, 4 pre-existing tests ignored; 188 Python tests passed (deploy 125, randomized search 3, tactical tree 22, NNUE 38).

- `cargo-fmt`: passed (0.57 s). [Log](checks/final-cargo-fmt.log.gz).
- `clippy`: passed (17.38 s). [Log](checks/final-clippy.log.gz).
- `cargo-test`: passed (45.64 s). [Log](checks/final-cargo-test.log.gz).
- `ruff-format`: passed (0.07 s). [Log](checks/final-ruff-format.log.gz).
- `ruff-check`: passed (0.02 s). [Log](checks/final-ruff-check.log.gz).
- `deploy-tests`: passed (23.97 s). [Log](checks/final-deploy-tests.log.gz).
- `randomized-search-tests`: passed (0.76 s). [Log](checks/final-randomized-search-tests.log.gz).
- `tactical-tree-tests`: passed (0.29 s). [Log](checks/final-tactical-tree-tests.log.gz).
- `nnue-python-tests`: passed (44.73 s). [Log](checks/final-nnue-python-tests.log.gz).
- `cargo-fmt-check`: passed (0.45 s). [Log](checks/final-cargo-fmt-check.log.gz).
- `ruff-format-check`: passed (0.01 s). [Log](checks/final-ruff-format-check.log.gz).
- `ruff-check-final`: passed (0.01 s). [Log](checks/final-ruff-check-final.log.gz).
- `diff-check`: passed (0.01 s). [Log](checks/final-diff-check.log.gz).

Rust checks use CI-pinned Rust 1.97.0; Ruff uses CI-pinned 0.16.8. Python lint covers all tracked Python plus the new runner/tests, excluding pre-existing user untracked analysis/backups. NNUE Python tests use the existing `data/nnue-venv`. All measured performance binaries use Rust 1.92.0/LLVM 21.1.3 to match the baseline compiler. Official temporary tool downloads are hash-verified in the provenance records.

Regression coverage includes repetition collision counts, 127/128 history threshold, twofold/fivefold transitions, null moves, independent cloned suffixes and undo; Range forward/diagonal/edge/blocker/sweep/raw-count behavior; mixed-capability output-prefix retention; all 256 direction masks; full-width NNUE sums and adversarial VNNI lanes; TT capacity/addressability; unbalanced bootstrap weighting, shared-game clustering, singleton intervals, eager-loaded input verification and SIGTERM worker cleanup.

Limits: one laptop/CPU, convenience game corpus, shallow fixed depths, few source-game clusters, no hardware-counter profile, no cross-architecture performance validation, no new tactical-strength or Elo experiment, and no cold-start measurement. Even exact faster code can choose a different move under a clock. The inherited trusted JSONL watchdog waits for initial readability and assumes the helper emits a complete line.

## Reproduce

Build baseline and candidate in separate clean checkouts with the same native command and compiler. Confirm `rustc -Vv` reports Rust 1.92.0/LLVM 21.1.3 before building to reproduce this compiler match:

```sh
./deploy/build_native.sh --offline --release --example nnue_tournament_bench -j 4
```

Then run the versioned paired runner against immutable copies of those executables:

```sh
python3 deploy/benchmark_search.py \
  --baseline /path/to/baseline --candidate /path/to/selected \
  --baseline-revision caf49f64a5a6eb2a1ba2660040676217bc1ce794 \
  --candidate-revision 480393c630d4fac476ec0df53d9e19bb4c78c9cc \
  --corpus benchmarks/engine_speed_20261030/corpus.json \
  --cases benchmarks/engine_speed_20261030/confirm-cases.json \
  --repeats 6 --aa-repeats 2 --cpu 2 --out /tmp/new-engine-speed-confirm
```

The archived corpora reference local game/checkpoint/network assets by absolute path and hash; they are not bundled. Supply the same assets or deliberately create and record a new corpus. Consecutive-root validation uses sequence-corpus/cases, `--sequential` and four native repetitions.

PGO uses the existing workflow and a reduced corpus containing only three training roots, preventing eager startup replay from profiling reserved games:

```sh
CARGO_BUILD_JOBS=4 ./deploy/build_pgo.sh \
  benchmarks/engine_speed_20261030/pgo-training-corpus.json \
  data/derived/new-engine-speed-pgo /path/to/matching/llvm-profdata \
  --positions 0 1 2 --models 0 1 4 5 6 --repeats 10 --depth 2 --cpu 2
```

Use an llvm-profdata version matching the benchmark rustc LLVM version. Profile models include narrow/wide NNUE and both handcrafted families. Reserved roots and the new sequence games are physically absent from the profiling corpus. PGO is compiler training, not NNUE training; instrumented profile times are not speed measurements. [PGO build metadata](pgo-build.json), profile workload records, binary/profile hashes, raw paired timings, warmups, original run plans and final clustered summaries are archived. [manifest.json](manifest.json) provides compressed-file and uncompressed-content SHA-256 hashes.

### PGO profile coverage

The profile-use build succeeded and emitted missing-profile-data warnings for additional CLI/HTTP functions. The saved [build log](checks/pgo-build.log.gz) contains no hash-mismatch diagnostic. This training workload does not establish whole-application coverage or performance. Instrumented searches are excluded from speed measurements; the separate paired comparisons above determine the measured PGO result.

## VPS rollout — October 31, 2026

The destination AMD EPYC-Genoa VPS selected the accepted mechanical speedups and host-trained PGO for **future games**, preserving its existing custom G3 search policies. Seven source files were backported from accepted revision `480393c630d4fac476ec0df53d9e19bb4c78c9cc`; the exact live-source content identity changed from `075f4024499f0fc0decdaee438af88d1eec2822e9d2b484ae27877b2c043e285` to `5b859b1576f93e5306ba11b9b7740f8fb5b36e989a23fee2f37f165f385b0a06`. Legacy F eligibility and G3 trial bodies were preserved. This was a compatible backport, rather than deployment of main's broader search changes.

On that host, **PGO versus the same-source native build used 9.54% less search CPU**: equal-case geometric-mean CPU fell from **13.6766 ms to 12.3714 ms**. There were 240 measured pairs: ten agents, six reserved roots from three games and four balanced repetitions, plus 60 matching warmup pairs. Every complete search signature matched. The 95% source-game-cluster ratio interval was **0.88707–0.91678** (3,000 resamples, only three game clusters). All ten per-model aggregates improved, by 3.35%–13.00%; the machine-readable record retains their actual before/after values and sample counts.

Compiler training used three different games, six roots, the same ten checkpoints, depth two and two repetitions. Both builds used Rust 1.97.1/LLVM 22.1.6 and host-native flags; matching LLVM profiling tools were checked. Verification timed user+system process CPU inside warm depth-two searches, excluding loading/replay. The eight-worker tournament and native publication checks remained active on the host. These are bounded same-source PGO results, with no isolated CPU lease, cold-start, realistic-clock or playing-strength measurement. They do not establish the VPS's total gain against its older deployed engine and must not be multiplied by the laptop percentages above.

Before native publication, all 86 production-analyzer comparisons matched every completed iteration's depth, score, full route and cumulative nodes: 60 reserved model/root combinations plus all 26 G3 policies on one reserved root. The backport passed 476 Rust tests (four ignored). The separate clean-main PGO tooling passed 531 Rust tests (four ignored), 275 Python tests and the required formatting/lint checks; the frozen live source retains legacy Clippy diagnostics.

Native and PGO publication each validated all nonhistorical registered models through the standard immutable updater with an expected-active-build guard. Coordinator PID 1170015 stayed unchanged, and completed results were preserved. The post-publication snapshot had 5,822 completed games; all eight already-running games retained their original binaries. PGO build `cab94ec05c57ae053bfa4362d39ee9a2b39707b57ca061eea0622e59c3dad458` was selected for subsequent games. The native build `05c1968b8532d7b6c762512a201d11d1476478381dcbb6beda0c3f16559d6596` and original bundle remain available for rollback.

[VPS measurements and deployment record](vps-pgo-20261031.json) include source, compiler, binary and merged-profile identities, per-model measurements, sample counts and limitations. The reusable tooling is revision `17f0c2b07e2af12f56b9b68febe3fa89d427e80a`. Raw pairs, profiling inputs, profiles, build logs and publication receipts remain in the VPS's durable `taikyoku/pgo/speedup-20261031` evidence directory; no game records or NNUE weights were copied into this report.
