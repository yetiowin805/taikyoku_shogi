# NNUE follow-up results — 20 September 2026

Packed byte arithmetic and the royal-probe bypass are the strongest candidates from this batch. All four exact additions together preserve the verified fixed-depth search results. The smaller-head ablations introduce measurable score distortion and are not ready for adoption from these tests. No tournament or checkpoint was changed.

## Confirmation: just packed arithmetic + royal bypass

An independent follow-up removes the low-value activation and memoization additions. It uses the same frozen binary and positions, 24 depth-2 verification runs and 48 fresh three-second timed runs (legacy, framework control and the recommended pair, two shuffled repetitions). Fixed-depth results remain exact.

- W512: +11.0% equal-position NPS and +7.8% pooled NPS versus the accepted binary. Repeat-specific aggregate gains: +10.5% and +11.6%.
  - opening: +0.3%; depths [3, 3] → [3, 3].
  - middlegame: +5.3%; depths [3, 3] → [3, 3].
  - tactical: +35.8%; depths [1, 1] → [1, 2].
  - long-history: +6.0%; depths [4, 4] → [4, 4].
- W2048: +33.8% equal-position NPS and +21.2% pooled NPS versus the accepted binary. Repeat-specific aggregate gains: +32.6% and +35.0%.
  - opening: +7.6%; depths [3, 3] → [3, 3].
  - middlegame: +5.6%; depths [3, 3] → [3, 3].
  - tactical: +161.7%; depths [1, 1] → [2, 3].
  - long-history: +7.9%; depths [2, 2] → [2, 2].

This smaller combination is the recommended candidate. The main batch had an anomalously slow W2048 middlegame exact run, so its headline was not the sole basis of the recommendation. Keep the original result rather than deleting an inconvenient observation; the independent confirmation and per-position results show the practical range.

## Original nine-variant batch

The reference is the preserved accepted native + fused-row-cache + snapshot binary, not the old generic build or a handcrafted evaluator. Percentages below are additional throughput. The equal-position result is a geometric mean of paired nodes/second ratios across four saved positions and two shuffled repetitions per width; it is not an estimate of tournament-wide strength or game completion rate.

### Width 512

- control: +1.6% equal-position NPS; +1.2% pooled NPS.
- quant: -1.7% equal-position NPS; -2.0% pooled NPS.
- packed: +3.3% equal-position NPS; +5.0% pooled NPS.
- royal: +6.6% equal-position NPS; +2.2% pooled NPS.
- memo: +0.3% equal-position NPS; -1.4% pooled NPS.
- exact: +9.3% equal-position NPS; +6.4% pooled NPS.

- All exact additions, middlegame: +8.5%; completed depths [3, 3] → [3, 3].
- All exact additions, opening: +0.4%; completed depths [3, 3] → [3, 3].
- All exact additions, long-history: +1.4%; completed depths [4, 4] → [4, 4].
- All exact additions, tactical: +29.2%; completed depths [1, 1] → [2, 2].

### Width 2048

- control: +0.3% equal-position NPS; +0.1% pooled NPS.
- quant: -1.2% equal-position NPS; -2.1% pooled NPS.
- packed: +6.5% equal-position NPS; +8.4% pooled NPS.
- royal: +26.7% equal-position NPS; +14.4% pooled NPS.
- memo: -0.2% equal-position NPS; -0.6% pooled NPS.
- exact: +33.5% equal-position NPS; +20.8% pooled NPS.

- All exact additions, middlegame: -1.9%; completed depths [3, 3] → [3, 3].
- All exact additions, opening: +8.7%; completed depths [3, 3] → [3, 3].
- All exact additions, tactical: +178.4%; completed depths [1, 1] → [3, 3].
- All exact additions, long-history: +6.9%; completed depths [2, 2] → [2, 2].

`control` measures the experimental framework without its new optimizations. `exact` combines quant, packed, royal and memo. Framework overhead and ordinary timing variation therefore remain visible; changes of a few percent are inconclusive in this small sample. Total valid timed searches including confirmation: 192. Comparing with the untouched binary avoids presenting framework overhead as an optimization win.

## Why the changes differ

- W512: packed output-head inference is 2.26× the speed of the accepted binary in the isolated microbenchmark. In the tactical depth-2 search, 770,742 royal probes cause accumulator change calls to fall from 2,143,882 to 97,886 with the bypass (95.4% removed). Residual caching hits 15/31,535 calls (0.048%).
- W2048: packed output-head inference is 2.34× the speed of the accepted binary in the isolated microbenchmark. In the tactical depth-2 search, 403,502 royal probes cause accumulator change calls to fall from 1,115,904 to 43,030 with the bypass (96.1% removed). Residual caching hits 20/19,828 calls (0.101%).

The royal bypass has little reason to help a position without these probes. The packed kernel is much faster locally, but move generation, attacks, make/unmake and accumulator maintenance limit its whole-search impact. The activation conversion removes unnecessary wide arithmetic but contributes little by itself. Memoization needs additional state/invalidation machinery for almost no observed reuse.

## Evaluation-quality tradeoffs

These prune the first dense layer from 32 calculated neurons to 24 or 16; the 512–2048-wide feature accumulator and its large table do not shrink. Removed activations take calibration means. Weights are frozen: this is neither retraining nor a test of the best possible smaller network.

Calibration used 256 positions from 256 training games. Validation used 512 positions from 124 separate games at all five widths. These games were held out from pruning calibration, but the original nets had used the validation split for early stopping. Search-score teacher labels are imperfect; MAE changes below do not imply equivalent Elo changes.

- W512, 24 active neurons: mean absolute score change 124.0, p95 378, maximum 639; teacher MAE +0.52% (95% paired game-bootstrap interval -1.51% to +2.69%).
- W512, 16 active neurons: mean absolute score change 336.7, p95 1057, maximum 2278; teacher MAE +7.37% (95% paired game-bootstrap interval +1.59% to +13.78%).
- W768, 24 active neurons: mean absolute score change 149.7, p95 446, maximum 1310; teacher MAE +3.12% (95% paired game-bootstrap interval +0.36% to +6.10%).
- W768, 16 active neurons: mean absolute score change 418.5, p95 1389, maximum 3047; teacher MAE +15.07% (95% paired game-bootstrap interval +8.24% to +23.18%).
- W1024, 24 active neurons: mean absolute score change 171.9, p95 502, maximum 1440; teacher MAE +3.59% (95% paired game-bootstrap interval +0.65% to +6.92%).
- W1024, 16 active neurons: mean absolute score change 403.5, p95 1284, maximum 2288; teacher MAE +10.57% (95% paired game-bootstrap interval +3.68% to +18.27%).
- W1536, 24 active neurons: mean absolute score change 189.4, p95 485, maximum 1980; teacher MAE +2.80% (95% paired game-bootstrap interval -0.17% to +6.32%).
- W1536, 16 active neurons: mean absolute score change 379.0, p95 1078, maximum 3058; teacher MAE +8.81% (95% paired game-bootstrap interval +2.03% to +16.20%).
- W2048, 24 active neurons: mean absolute score change 154.9, p95 487, maximum 1048; teacher MAE +2.74% (95% paired game-bootstrap interval +0.39% to +5.31%).
- W2048, 16 active neurons: mean absolute score change 312.0, p95 874, maximum 1760; teacher MAE +7.02% (95% paired game-bootstrap interval +2.22% to +11.86%).

These use native evaluation units, not chess centipawns. The quarter-head reduction changes total-evaluation sign in 39/2,560 model-position checks; half-head reduction does so in 92/2,560. Most are near zero. Correlated checks across widths must not be treated as independent games.

- W512, head 24: additional NPS +0.6% versus all exact additions; isolated output-head throughput +8.7%; changed chosen moves in 2/12 fixed-depth checks.
- W512, head 16: additional NPS +1.6% versus all exact additions; isolated output-head throughput +15.2%; changed chosen moves in 3/12 fixed-depth checks.
- W2048, head 24: additional NPS +9.6% versus all exact additions; isolated output-head throughput +8.1%; changed chosen moves in 3/12 fixed-depth checks.
- W2048, head 16: additional NPS -2.0% versus all exact additions; isolated output-head throughput +30.7%; changed chosen moves in 3/12 fixed-depth checks.

Changed-move checks use four depth-2 saved positions and eight additional depth-1 validation positions per tested width, chosen before inspecting pruning outcomes. Because pruning changes evaluation and therefore search work, NPS is not an equal-work comparison for those variants. A faster or deeper nominal search is not evidence that its moves are stronger. Full original/variant moves and search scores are retained in `followup-summary.json`.

## Validation and limits

- The initial batch has 144 valid timed searches, 72 depth-2 comparisons, 48 additional depth-1 quality searches, 14 microbenchmark processes and 32 separately instrumented searches.
- All seven exact/control variants match fixed-depth static/search scores, full selected routes, ordered root lines, node counts and quiescence counts in all eight width/position cases.
- Seven targeted NNUE tests pass in release and debug with all exact flags enabled; these cover arithmetic extremes, packed sums, collisions, snapshots, model changes, clones, promotion, timeout recovery and cached residual invalidation/restoration.
- Twenty Rust/NumPy checks agree for the pruned networks, covering all five widths and both pruning plans.
- Eight additional depth-3 comparisons on a synthetic double-check fixture preserve exact search results. All-target feature-enabled checking and the seven targeted debug tests also pass.
- Default `cargo check --offline` passes. Full production validation is still required before enabling the prototypes.
- Timings were sequential on local CPU 0, i7-1255U, native release build with thin LTO and one codegen unit; no intentional competing build/workload. This does not establish VPS performance.
- Peak RSS in the main batch stays approximately 276 MiB at W512 and 999 MiB at W2048; the exact combination adds less than 0.5 MiB versus the accepted binary.
- Model load, initial replay and initial accumulator construction are outside search timers. RSS includes the entire process. Memoization is disabled for isolated forward microbenchmarks.
- An initial stale-executable batch was discarded and preserved under `invalid-old-binary/`. All results here use the corrected binary and validated follow-up telemetry.

## Decision

- **Promising exact candidates:** packed output-head arithmetic and royal-probe bypass. Keep both opt-in until production integration and VPS checks.
- **Small/inconclusive:** the 32-bit activation conversion; a clean exact simplification, but no strong standalone search-speed claim.
- **Hold out:** residual memoization in this form. Its observed hit rate is too low to justify the added cache state.
- **Quality tradeoff, hold out:** post-training head pruning. The 24-neuron head is a more plausible retraining/distillation experiment than the 16-neuron head, but neither demonstrates strength parity here.
- **Untested in this batch:** fully lazy/fused child accumulators, warm row caches, further packing/tiling, skipping unused handcrafted bookkeeping, narrow accumulators and page tuning.

See `FOLLOWUP_EXPERIMENTS.md` for reproduction and `followup-results.json` for compact data. Full raw results, plans, model/game/source/binary hashes and the discarded-batch audit are under `data/nnue-followup-experiments-20260920/`.
