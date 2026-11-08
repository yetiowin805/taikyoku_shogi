# V6 automatic training handoff

`v6_pipeline.py CONFIG` waits for a completed collector status, every planned
collection stage, zero unfinished/failed catalogue jobs, and release of the
collector lock. It then prepares one immutable dataset and runs four regimes at
384 and 512 widths. It never admits a model to a tournament.

The VPS service is `taikyoku-nnue-v6-training.service`. Its environment file is
`/etc/taikyoku/nnue-v6-training.env` with absolute `PIPELINE` and `CONFIG` paths.
Configuration pins code, binaries and parents; includes the catalogue, collection
config, control directory, Python interpreter, output directory, parent paths,
and CPU leases `[7,1,3,4,5,6]`. The six-CPU service cap is shared by preparation
and trainers. Each subprocess borrows exactly one game-boundary lease and uses
one Torch thread. No CPU is borrowed while collection is incomplete. During
maintenance trainers checkpoint at effective-batch boundaries and release leases.
Dataset preparation finishes its current preparation job before yielding its CPU;
an explicit service interruption reruns incomplete preparation.
There is no overall runtime limit or fixed maximum number of training windows.

## Dataset and evidence

Only completed finite root-search observations are eligible. Each distinct
teacher family contributes its largest available budget once. Unverified mate
claims and terminal positions are excluded, rather than treating mate integers
as scores. The collection does not provide forcing-line proofs; this first
implementation does not manufacture verified mate targets or add a saved-label
supplement. Outcomes are mixed only for replay-verified terminal game results;
alternative children never inherit the original game outcome.

A symmetric teacher temperature is fitted to representative training outcomes
with equal game weight over 33 candidates from one quarter to four times the
established scale. Fewer than 100 verified games or a boundary optimum uses the
existing scale, explicitly recorded. Student output scale stays fixed at
2822.9057434005167. Test data does not affect calibration, optimization or selection.

Feature duplicates across splits are removed globally. Fresh holdouts also need
the collector's union-of-parent-exposure check. Conflicting duplicate labels
(probability range > 0.15) are withheld; compatible duplicates receive one
averaged target. Group splits remain unchanged. Feature export replays the exact
root and legal branch, checking feature/schema/history hashes, turn and material.
Both parents must have the same material table. All artifacts are hashed before
publishing the dataset completion marker. Incomplete preparation can be rerun.

Sampling mass is 75% representative, 20% challenge, 5% alternatives, redistributed
when unavailable, then balanced by related-game group, game, phase and distinct
feature row. Sampling probabilities are not multiplied into the loss again.
Confidence normalization is separate and used only by C. `audit.json` records
exclusions, calibration, split/group counts and pair count.

The existing 92-channel ability schema is preserved for warm starts. Piece and
promotion identities that share these features remain indistinguishable; this
experiment does not claim to resolve that architectural limitation. No piece
values are hand-reduced to compensate for the corrected range-two-mover rules.

## Shared conversion and regimes

Parents are 384v4.5 and 512v3. Each width has two matched four-window preludes:
A retains material with alpha 1; B uses alpha 0.75, 0.5, 0.25, 0. Each window has
65,536 weighted draws by default, effective batch eight, microbatch one. A/B use
the same BCE objective and seed during the prelude. Conversion stops with an
explicit error if a fixed training-only guard sample's teacher Brier worsens by
more than 0.05 against the initial parent. Alpha-positive states are never exported
as total-score models. B/C/D share the final B prelude weights, with fresh optimizer
state at the regime boundary; A starts from its equal-exposure prelude.

- A: residual plus fixed material; BCE against 85% teacher / 15% eligible outcome.
- B: total score; the same BCE and outcome mixture.
- C: total score; confidence-weighted `0.8*0.25*Huber(logit(p)-logit(q)) +
  0.2*2*(p-q)^2`, with teacher logits clipped to probabilities 0.01–0.99.
  Confidence has a 0.25 floor, using teacher spread and within-family budget drift;
  single-teacher labels use 0.75. Mean confidence is normalized within each stratum.
- D: B plus 0.10 sibling-ranking BCE, sampled uniformly by root then pair. Both
  sibling scores use a common Black perspective. Pairs need a probability gap of
  at least 0.10, two distinct teacher families, two budgets, and consistent ordering
  in every shared observation. These are independent child-search values, not
  heuristic root candidate scores. There is no additional quietness filter in this
  first implementation. Zero usable pairs blocks D rather than silently making it B.

All objectives share the factor `4*(scale/1000)^2`. Sparse embedding SGD peaks at
1e-4, dense-head Adam at 1.5e-4, with a 32,768-draw warmup from 10% to 100%.
A common validation criterion is 85% group/game-balanced teacher Brier plus 15%
verified-outcome Brier, using representative validation only. If no eligible
validation outcomes exist, all runs use teacher-only selection. A minimum of four
windows, three unimproved windows (0.5% relative threshold), and two learning-rate
halvings precede plateau stopping. This replaces the report's proposed cosine and
fixed draw ceiling to honor the requested convergence-based unlimited runtime.
Test data remains untouched for subsequent confirmation.

The quantized forward pass uses existing sparse NNUE machinery. Export checks
integer-reference score parity on up to 64 validation examples. The descriptor
`output_mode` and binary baseline flag agree: missing mode/default flag 1 means
legacy residual; `total` requires flag 0. Older engines reject total models rather
than double-adding material. Search material heuristics and terminal rules remain
unchanged. Use this revision's engine for evaluating v6 candidates.

## Recovery and outputs

Each run has immutable recipe, status, metrics, complete checkpoint and completion
marker. Graceful pauses save optimizer, window offset, sampler state and conversion
guard baseline at batch boundaries. Abrupt termination replays only work since the
last committed checkpoint. Atomic model export and complete markers make restart
idempotent. Finished optimizer files are removed; parent, prelude start weights,
metrics and all eight inference models remain.

The pipeline service restarts unexpected process failures, with five starts in
five minutes. A failed child or dataset prerequisite writes `blocked.json` and
prevents expensive repeated retries; inspect the per-job log and deliberately
clear the block only after addressing its cause. `ready.json` lists the eight
candidates only after all runs complete and their model hashes/modes validate.
This is training readiness, not a claim of playing strength or automatic promotion.
