# NNUE search-efficiency experiment protocol

Control: current main `99f8c9b3d4c4c9fc70e1ee58d362e249310cb5a6`, W512 v4.5 network `81b1c69ab269ecce313a93b0cbe866ede18c37d8a5ff83116dc20d196ba12ea4`. The descriptor and complete game records are hash checked. Missing NNUE fails; no handcrafted fallback. Structural unit tests use small synthetic fixtures; measured performance uses the actual NNUE checkpoint.

The source games and initial 110-position manifest come from local study commit `df1ecb3`; the game archive remains local. Search positions retain the complete game prefix, not a board-only reconstruction. They are exploratory positions already exposed in the earlier study, not an untouched playing-strength test. Frozen requests and binary hashes are in each `results/*-plan.json`.

## Questions and acceptance criteria

1. Can lazy royal-capture detection, an earlier existing large-victim filter, or cached quiescence sort keys reduce CPU time without changing the search? Require exact chosen full route, score, completed depth, total/q nodes, q generated/searched counts, q TT hits and reported root lines on non-aborted fixed-depth pairs. Same result is the expected outcome, not evidence of no benefit.
2. Does interior principal-variation search improve allocation near depth 5? Compare fixed-depth work and actual completed depth/choices under a common time limit. Include the safeguard that PV nodes do not use non-PV TT scores as substitutes for searching their longer q horizon. Measure that safeguard separately before attributing overhead to PVS.
3. Which part of quiescence preparation consumes time? Use a separate instrumented executable. Positive-budget q evaluation, generation, annotation and sort timers are disjoint local spans. Existing global evaluation/generation/attack timers overlap with those spans; do not add all reported buckets together. Timer instrumentation affects timing, so these runs establish approximate attribution, not speedups.

## Measurement

Intel Core i7-1255U, one search worker pinned to logical CPU 2, nice 10, native optimized Rust release build. Loading, replay, legality generation and first depth-1 warmup are outside the measured search. Search state/model caches are warmed in a single persistent process; search TTs are fresh/cleared for each request. No compilation or second benchmark runs concurrently with timed experiments. Other desktop load cannot be controlled completely.

The initial screen uses 12 positions at depth 3, seven variants, one pass. It is for correctness and triage only. Follow-up repeats use deeper fixed work, randomized position order and interleaved candidate/control order. Report per-position median paired CPU ratios and their geometric mean; do not count repeats as independent tactical positions. Capped/incomplete searches are excluded from fixed-work speed ratios and separately reported. Fixed-time comparisons retain incomplete iterations and actual completed depths.

No Elo or general strength claim follows from these measurements. Game outcomes, rare missed defenses and broader independent coverage would be required to establish a stronger playing agent. A full integer ply gain is not a prerequisite for retaining a reliable behavior-preserving speedup.

## External backing for the mechanisms

- [Stockfish move picker](https://github.com/official-stockfish/Stockfish/blob/master/src/movepick.cpp): staged TT/capture/quiet selection and precomputed move scores motivate reducing preparation work. The current experiments preserve our own eligibility and ordering; they do not copy chess-specific constants.
- [Stockfish search](https://github.com/official-stockfish/Stockfish/blob/master/src/search.cpp): narrow-window scouts, re-searches and PV/non-PV handling motivate interior PVS. Our different q horizons and incoming capture context require explicit care.
- [YaneuraOu search](https://github.com/yaneurao/YaneuraOu/blob/master/source/engine/yaneuraou-engine/yaneuraou-search.cpp): primary shogi precedent for selective quiescence and search. Its mechanisms are precedents, not evidence that a specific adaptation improves Taikyoku.

The evidence for every measured claim must be our own raw result rows and analysis script, rather than an external engine citation.

## Adaptive follow-up decisions

The first six profiles put q sorting at 0.01–0.51% of instrumented search time (0.24% when weighted by total search time), and annotation at 0.05–3.30%. Therefore deeper repetition focuses on the combined q changes, lazy royal probing separately, and cached stage-B ordering; individual early-filter/sort arms remain in the initial screen. No hypothetical large total gain is attributed to a sub-percent component.

PVS V1/V2 was preliminary. V3 also re-searches a PV scout that exceeds beta, because non-PV and PV q budgets differ, and restores incoming capture context for a first reduced move's re-search. It counts wide-window non-leaf nodes, to avoid interpreting root-only PVS as an entirely wide-window interior tree. V2's separately saved depth-4 rows are not the final evidence for the PVS decision.

The optional interval in `summary.json` is an exploratory 4,000-resample game-cluster bootstrap over per-position median paired CPU ratios (seed 45010). It describes these sampled cases, not strength, Elo, or performance on unobserved hardware.

The final five-second test uses a depth ceiling of 8, matching the clocked-search ceiling documented by the deploy scripts. Typical completed depth can remain around 4–5; the ceiling avoids hiding gains by stopping both arms at depth 5. This is a common hard-budget comparison, not a replay of the full Fischer soft/hard time manager. The ordinary next-iteration admission rule remains enabled: a search may stop before five seconds when the last iteration suggests that the next one cannot fit. The timed arms are control, PVS1 and PVS2, with two interleaved repetitions over 16 positions.

## Power-state incident and exclusions

The additional q holdout batch was interrupted after nine completed rows of 56 planned. A control/candidate pair took 22.1 versus 59.8 CPU seconds for almost the same node count; the latter hit its deadline. Inspection then found AC offline, CPU2 sampled at 700 MHz, and energy preference `power` (`results/host-power-snapshot.json`). The time of transition is unknown. All timing and deadline-based depth comparisons from this group are excluded, with the raw rows and the incomplete plan preserved. Three complete fixed-depth pairs still match exactly; this is a structural check, not a speed estimate.

The user reconnected AC before the separate fixed-time PVS batch. Its plan records the initial power state and runner hash; every result records before/after AC, frequency, energy-preference and package-temperature readings. The runner stops if AC is missing before a timed request. These spot readings cannot certify constant frequency throughout a search, but expose a repeat of the observed power-state confound. Earlier groups lack such telemetry, so their small timing differences remain provisional. The 144-row confirmation batch completed before the interrupted group; its per-position control max/min CPU ratio across three repetitions has median 1.084.

The additional q batch was not restarted: no candidate had established a convincing win in the completed confirmation, and the remaining budget went to the more consequential fixed-time PVS question. The shared VPS was already saturated with other engine jobs; no competing remote measurements or job changes were made.
