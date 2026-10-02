# Coverage audit against the original request

This distinguishes real-engine experiments, isolated cost measurements, and explicit deferrals. No retraining was performed. A supplied R² result is not a result we independently reproduced.

## Existing PR work already verified on real models

- Full attack-query hash memo and leaf-gate memo: real-network/handcrafted corpus checks in the previous PR study; not merely the previously rejected single-entry attack cache.
- Batched updates into undo storage, i16 fused rows with exact wide fallback, larger row-cache capacity, vectorized input quantization, whole-move delta cache, flat caches and cache persistence: retained supplied implementation bundle, independent arithmetic tests and real-network search checks in the previous study. This verifies the combined implementation; it does not reproduce every supplied per-change speed ratio independently.
- AVX-VNNI: implemented and executed on this host. AVX-512 VNNI: compiled but not executed; this CPU lacks AVX-512. Execution/performance validation needs a supported machine.
- Smaller row/delta caches: previous real-engine experiment, including consecutive positions. Not newly rerun as a separate factorial sweep.

## Real-engine experiments in this follow-up

- Host CPU compilation versus generic, PGO, fat LTO and fat-LTO/abort-on-panic combinations: root toolchain screens. Report measured combinations rather than claiming the original 18% aggregate transfers unchanged.
- Captures-onto-square result caching: full-route cache, movement-origin-aware key, current piece-list order reconstruction; focused test and root exact screens.
- Direct promotion-zone generation: extra ray pruning beyond already present direct promoted-variant emission; focused exhaustive-type/rank/orientation fixture and root exact screens.
- Staged TT move picker: behavior-changing full-engine d3/budgeted-d4 prototype, legal-route checks and raw move/score/node/depth comparison. No consistent clear win; no strength claim.
- Persistent main TT: behavior-changing game-prefix prototype, same measurements. Promising in some handcrafted positions but a clear NNUE counterexample; needs owned model/config/game session and repetition-history handling. Q bounds remain fresh. Removing the draw counter is intentionally not done: it changes progress-draw semantics and is not needed for actual next-position reuse.
- Position/residual cache: exact quantized-input-verified implementation, real-model timing, plus repeated-position census. Regressed and rejected.
- Deferred accumulator updates: actual lazy engine prototype, nested/undo/model tests, census of truly unconsumed updates, real-model timing. The census ceiling is not reported as achieved speedup.
- Share fused rows between perspectives: actual canonical-feature-identity cache prototype, invariant tests and engine screens. This is not unconditional geometric mirroring; differing feature lists/origins stay distinct.
- Huge pages: actual best-effort weight-only and weight-plus-cache advice prototypes, engine screens and host coverage/RSS checks. No global OS reconfiguration.
- Prefetch capture rows: actual prototype after move-delta-cache misses; memory agent/root finish its engine screens. A prefetch instruction alone is not evidence of a win.
- Dense head layout: exact transposed AVX-VNNI four-input kernel with arithmetic parity and full-engine screens. This is a dense-layout optimization, not a sparse-inference claim.
- Additional discoveries: reuse the already built root accumulator; exact compressed move-delta payloads with i32 fallback, including a fused narrowing/fit-check refinement. The latter does not make accumulator sums i16.

## Measured enough to justify skipping a full engine variant

- Material-only evaluation cutoffs: real-network rigorous-bound census found 17 eligible cases in 166,395 residual calls and no exact clamp shortcuts. A cutoff proof still changes fail-soft returned/stored scores if evaluation is omitted. The tiny opportunity plus logic change justifies not implementing a full shortcut experiment now.
- Zero-skipping/sparse first layer: earlier real-net activation sampling found fewer than 0.6% zero lanes and prior sparse/transposed microbenchmarks gave no consistent benefit. Current follow-up tests the useful dense-layout idea separately. A full-engine zero-skipping kernel is not newly claimed tested; revisit if newly trained nets are much sparser.

## Isolated synthetic cost measurements only; full architecture evaluation deferred

- Pairwise products, 16-wide first layer, their combination, PSQT shortcut, pairwise+16+PSQT, single perspective, single+pairwise+16+PSQT, and narrower widths 1024/512 versus 2048: root's generated-weight microbenchmarks measure dense-head and cached-update cost. They do not measure trained accuracy, playing strength, actual search-tree effects, model loading, or all production memory traffic. No architecture is ready to ship on those numbers alone.
- Unconditional i16 accumulator sums: not valid for every currently accepted network (large permitted weights/biases). Real-position sum census fits i16; root measured an overflow-guarded narrow-update kernel. There is no full-engine narrow/wide accumulator-and-snapshot implementation or end-to-end claim. A clipped training/model contract requires retraining; exact fallback remains a later engineering option if its guarded-kernel result warrants it.
- Regional block accumulators and global/regional hybrid: not independently benchmarked or retrained in this follow-up. Explicitly defer because the supplied proxy reports a large accuracy loss and exact move-delta caching already removes much of their proposed update advantage. Those proxy losses are reported evidence, not newly validated facts.
- Small leaf network and network switching in lopsided positions: direct-leaf opportunity is measured (77.9–90.2% conservative residual-call share), but no small model, switch criterion, or playing-strength evaluation was trained/tested. Random-weight search would not establish whether choosing that model is safe. Defer until trained candidate weights and game comparison are available.
- Purely linear positional model: the supplied proxy's high R² was not retrained/revalidated, and no pure-linear engine candidate was tested. PSQT's isolated shortcut cost is measured, but accuracy/strength of a linear model remains deferred; fitting the mostly-linear handcrafted teacher does not prove game strength.
- Widths 64/128/256 accuracy and all quoted held-out R² rankings: not reproduced, per the user's no-retraining constraint. Cost samples at other widths and existing trained networks do not validate those accuracy claims. Trained 1024-versus-2048 matched game comparison also remains deferred.

No original major idea should disappear from the final ledger: the genuinely unimplemented ones are the explicit deferrals above. The original precise NNUE time-share/"free network" ceiling is not independently established by intrusive sampled counters; do not present the old 25–30% / 1.35× figures as a newly verified end-to-end result.
