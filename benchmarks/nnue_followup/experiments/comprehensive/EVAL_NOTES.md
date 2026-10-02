# Evaluation work: isolated experiments from PR #137 (`903986e`)

These are experimental patches, not a recommendation to ship all variants. Root task owns end-to-end timings and production decisions. No retraining.

All definitive binaries must be built after package-clean in the shared target directory; two early normal builds were found to be stale Cargo artifacts from another source export and moved to names beginning `invalid-stale-`. They must not be benchmarked or used as evidence. Definitive source hashes and patches are written by `variants-build.sh`; all builds/tests execute under `/tmp/nnue-comprehensive/cpu.lock`, offline with `RUSTFLAGS=-C target-cpu=native`.

## Exact residual cache (`cache.patch`, `bin/eval-cache`)

Thread-local, direct-mapped 4096-entry cache. Index/key uses the incremental board hash with the progress counter XORed out. Every hash hit verifies the entire quantized, side-to-move-ordered accumulator input. Thus even hash collisions or different positions with equal quantized inputs preserve the exact residual. Weak model identity prevents stale model hits without retaining the network. Terminal/progress-draw handling and material calculation remain outside the cache, and only the NNUE residual is cached; handcrafted ply-dependent evaluation is untouched.

Memory payload is up to `4096 * 2 * width` bytes (16 MiB at width 2048), plus keys/vector metadata. Quantization still executes on cache hits, and misses copy the full input. It may regress when repeats are scarce. A dedicated unit test is included for same-key unequal inputs and changed model identity. The initial test command was also affected by shared-target artifact reuse and did not execute that new test; its log is marked `invalid-stale-cache-tests.log`. Fresh fixed-depth signature checks and timing come from the root task. If this variant is retained, rerun the dedicated test from a clean package build before shipping.

## Deferred accumulators (`lazy.patch`, `bin/eval-lazy`)

An isolated wrapper around the original eager accumulator owns deferred move changes and frame snapshots. A residual request materializes every unconsumed ancestor in order, using the existing exact row/delta cache. Undo of an unconsumed frame performs no accumulator arithmetic. Undo of a consumed frame restores its exact original snapshot. The wrapper moves the already-created piece-change vector into the frame, avoiding a second copy. It preserves cloned states, promotion/sweep deltas, rule-only probes that temporarily remove NNUE, null-side changes, and model-switch invalidation.

This implementation adds frame/interior-mutability overhead; a small skip percentage alone does not establish a net benefit. Targeted tests exercise nested descendants, cloning before evaluation, make/unmake, unchanged root sums, and existing overflow/model-switch cases.

## Diagnostic census (`census.patch`, `bin/eval-census`)

`run_census.py` must be called under the shared CPU lock. Each JSON row includes `census`:

- `evals`: residual calls, including root/static calls; this is the denominator for `position_repeats` and the conservative `leaf_evals` count.
- `position_repeats`: repeated network-pointer/board-hash keys after removing the progress counter, within one benchmark request. A diagnostic 64-bit hash count, not the exact production cache hit rate; no persistent cross-request repeats are included.
- `moves`, `changes`: accumulator save/apply calls and changed-piece counts.
- `unused_moves`, `unused_changes`: moves whose update is never consumed by that node **or any descendant** before undo. This is a tighter lazy ceiling than simply counting nodes that do not evaluate. Divide by moves/changes respectively. It does not credit speculative cross-move delta cancellation.
- `leaf_evals`: residual calls in restored frames that never made a child. Initial/root and cloned-away frames are omitted, making this a conservative direct-leaf count.
- `eval_lanes`, `eval_outside_i16`, `move_lanes`, `move_outside_i16`: all actual sum channels at evaluations and immediately before every ordinary undo. Min/max encompass both, with zero included conservatively. Search make/unmake discipline makes this cover every intermediate move result in the search, even if it never evaluates.
- `residual_samples`/`residual_ns` and `update_samples`/`update_ns`: one timing sample every 256 calls, at different fixed offsets. Scans/counters are outside measured sections. `ns / samples * total_calls` estimates only the respective routines' cost. Instrumentation changes cache state and branch behavior and may sample rare workloads unevenly; these are rough time-share estimates, not production benchmark speedups. Search wall times from the census must not be compared as performance results.
- `q_evals`: evaluated q nodes with qdepth > 0, after TT exits; `q_cutoffs` is the subset accepted by both existing royal/major-capture guards. `material_bound_cutoffs` is the subset whose rounded material plus the rigorous network lower bound already reaches beta. Divide by `q_evals` for the tested q-node opportunity or by all `evals` for the larger residual-call denominator.
- `clamp_shortcuts`: all residual eval calls for which network lower/upper bounds force the final mate_score/2 clamp to exactly one value; unlike fail-soft cutoff substitution, these could avoid NNUE while preserving the exact returned score.

Residual bounds follow the actual f32 output accumulation order, choosing each output weight's negative endpoint for the lower bound and positive endpoint for the upper bound. All final hidden activations lie in [0,1], so floating-point monotonicity plus final rounding/clamping makes these bounds conservative. Current trained networks have approximately [-5k, +6k] cp bounds.

## Why material-only cutoffs are not an exact-score optimization

Quiescence is fail-soft: it returns/stores the actual `stand_pat` score on a beta cutoff. A material+bound test proves that a cutoff is valid, but does not determine the original score. Returning beta or the lower bound would alter TT scores, later pruning and signatures. This remains a possible logic-changing experiment if the census establishes enough opportunity. It should not be silently merged into the exact-score speed patch. Exact output-clamp shortcuts are counted separately.

## i16 accumulator sums

Accepted networks allow biases up to 1,000,000 and weights up to 2,048, so unconditional i16 sums are not valid for the existing model format. Observed sums fitting i16 is evidence about these games only. An overflow-checked narrow representation with exact i32 fallback and correct narrow/wide snapshots **can** work without retraining. It introduces overflow checks, conversions/fallback allocation and representation switching; the root task is measuring an isolated guarded-update kernel. This work does not include a complete engine implementation and makes no end-to-end speed claim for it. The memory agent's separate compact move-delta variant is different: its accumulator and snapshots remain i32.

## Reuse the root accumulator (`root-reuse.patch`, `bin/eval-root-reuse`)

Newly discovered during this work: the normal search and quiescence probe computed their reported root static score against an immutable input state, constructing and dropping a fresh NNUE accumulator when one was not already bound. They then cloned that same state and built the same root sums again for search. This isolated variant prepares the NNUE clone first, reports its static score, and reuses it for search. Handcrafted evaluation retains its original setup order. Incremental material initialization and the old material fallback sum pieces in the same per-color order, so the static score arithmetic is identical. Terminal and progress-draw handling still runs through the existing evaluation guards. This should mostly matter for shallow searches and positions with many pieces; its end-to-end measurements are separate from deferred updates.

## Census outcome

The 36 real-network cases matched all prior production search signatures (baseline binary SHA256 `f158bf6919787077c62695bbddf549058bc0ec243622fffb02fa2823358c5c44`). Raw results are `census-v2.jsonl` and `census-v3.jsonl`; exact grouped counts are in `census-summary.json`, with verification in `census-validation.json`.

- Unconsumed updates: width-512 v2 10.90%, width-2048 v2 10.40%, width-512 v3 3.87%, width-1536 v3 5.44% of accumulator moves. The corresponding changed-piece savings are only 3.02–5.87%, so skipped updates tend to be smaller than average. These percentages are opportunities, not speedups; root task measures the lazy implementation directly.
- Repeated evaluation-position hashes within a request: 4.98%, 2.66%, 1.82%, and 1.96% respectively. Exact input-verified caching regressed in the root task's screens: v2 1.014×/1.032× search time, v3 width-1536 1.067×; width-512 v3 was effectively unchanged in total time (0.995×). Reject this cache implementation rather than spend more effort validating it for production.
- Material-only q-cutoff opportunity is negligible with rigorous bounds: 17 opportunities across 166,395 residual evaluations (16 at width 2048 and 1 at width 512 v2). No opportunity occurred in either v3 model. There were zero exact output-clamp shortcuts. Together with fail-soft score changes, this is strong reason to skip this idea for these nets.
- All 301,950,976 evaluation channels and 333,417,472 restored-move channels fit i16. Per-model min/max: v2 width 512 [-2251,4549], v2 width 2048 [-2137,6666], v3 width 512 [-3087,5776], v3 width 1536 [-3606,7023]. This also implies every observed move delta fits i16. These data do not prove bounds for all legal positions or accepted networks.
- Conservative direct-leaf residual counts were 77.9–90.2% per model. A small leaf network is therefore still a possible future architecture experiment, but changing the network requires strength/accuracy evidence unavailable without trained weights for that architecture.
- Sampled routine means: residual 0.71–0.75 microseconds at width 512, 1.59 microseconds at width 1536, 2.12 microseconds at width 2048; accumulator save/apply 0.84–1.20, 1.85, and 4.09 microseconds respectively. Treat these only as rough kernel observations under intrusive census instrumentation. Do not infer precise end-to-end fractions from them.
