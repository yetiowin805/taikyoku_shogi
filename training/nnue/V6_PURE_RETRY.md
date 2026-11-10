# V6 pure NNUE findings and retry plan

The completed 384-wide pure NNUE models learned the sampled training positions but failed to learn material values that transferred reliably to held-out games. The conversion removed fixed material on a predetermined schedule even as validation deteriorated, because its failure guard inspected training positions only. Missing promotion information is a separate representation defect that should be fixed whether or not pure NNUE is retried.

This document records the November 9, 2026 investigation and a proposed retry sequence. A retains fixed material; B and C are the original pure models; Cm is C retrained with fixed material. The measurements concern the completed 384-wide experiments, not an independently trained pure 512-wide model.

> [!IMPORTANT]
>
> **Required follow-up: include promotion possibility in the NNUE input.**
>
> Record whether a piece still has a future promotion available and what it can promote into. A native Free King and an already-promoted Free King must produce distinguishable inputs, even though their current movement abilities match. This is an open implementation task, not a completed fix.

## Findings

The strongest diagnosis is failure to generalize during conversion. This supports changing the representation, the way material is learned, and the acceptance criteria before trying longer training or another loss function. It does not establish the relative causal contribution of each factor.

### Good training fit concealed poor validation performance

All errors below are mean squared error between predicted and teacher probabilities; lower is better. These are not the combined teacher/outcome checkpoint-selection scores.

| Checkpoint | Sampled training error | Held-out validation error |
| --- | ---: | ---: |
| Parent 384v4.5 | 0.025743 | 0.024504 |
| A after preparatory windows | 0.005719 | 0.010016 |
| Pure B after conversion | 0.011686 | 0.046016 |
| 384v6A | 0.005421 | 0.009799 |
| 384v6B pure | 0.005470 | 0.043650 |
| 384v6C pure | 0.005385 | 0.044429 |
| 384v6Cm | 0.003975 | 0.008105 |

Training was measured on 4,096 weighted draws containing 3,843 distinct positions from 986 games and 457 related-game groups. Validation used all 3,721 representative validation positions from 119 games and 52 groups, with equal group weight and equal game weight within each group.

The sampling distributions differ, so the training/validation ratio alone is not a clean estimate of overfitting. The stronger comparison is B and C versus A on the same held-out positions: their errors are over four times larger. B's paired excess error over A is 0.03385, with a 95% group-bootstrap interval of 0.02701–0.04102. C's excess is 0.03463, interval 0.02752–0.04211. These intervals describe this validation cohort, not variation across new training seeds or tournament strength.

### The material ramp proceeded despite deterioration

The conversion used four windows of 65,536 draws, with remaining fixed material multipliers of 0.75, 0.50, 0.25, and zero. Validation teacher error rose through **0.01031 → 0.01591 → 0.02702 → 0.04601**. Main training recovered only a small part of that loss.

The guard evaluated 256 training positions and rejected only errors above the original parent's guard error plus 0.05. Its baseline was about 0.02507; the converted model scored 0.01421 and passed. Even eliminating the 0.05 allowance would not have caught this failure. Validation deterioration was recorded but did not halt or reverse the conversion.

B and C inherited the same converted starting point. Their different subsequent objectives therefore did not independently test whether conversion could succeed.

### Weak material sensitivity is consistent with passive play

On 128 systematically selected validation boards, 98 contained a qualifying enemy piece. The probe removed the highest-valued nonroyal enemy piece with an unambiguous old material value of at least 100, from both input perspectives, while keeping side to move fixed.

| Checkpoint | Median score response as a fraction of removed old material | Negative responses |
| --- | ---: | ---: |
| Parent | 100.1% | 0 of 98 |
| A | 100.0% | 0 of 98 |
| B conversion endpoint | 5.6% | 16 of 98 |
| B pure | 10.3% | 16 of 98 |
| C pure | 10.1% | 22 of 98 |
| Cm | 100.1% | 0 of 98 |

This was a synthetic structural check, not a legal capture or search benchmark. Removing a blocker can have tactical effects, and old material prices are not ground truth. Nevertheless, the systematic weakness is consistent with an evaluator giving insufficient dependable credit to material gains, allowing small positional preferences to dominate. It does not explain every quiet move individually.

### Identical inputs hide different promotion possibilities

The audit found 498 reachable base/promoted variants mapping to 226 distinct ability-channel lists. Two lists contain conflicting old material values.

A native Free King, priced at 1,620 by the old material calculation, has the same NNUE input as an already-promoted Free King priced at 80. The native piece can later promote to Great General; the already-promoted piece cannot. Several other queen-like pieces also share the encoding. Tengu and Capricorn share an encoding despite old prices of approximately 2,494.8 and 2,970.

The schema considers promotion when deriving current movement, but it lacks an independent channel that preserves promotion rights when those current abilities coincide. Exact reproduction of the old material function is therefore impossible on arbitrary boards with these inputs. The size of this defect's contribution to the observed training failure remains unmeasured.

The remedy is to expose the missing information. It is not to declare the old prices correct, particularly for range two-movers.

### A learned global value component is promising

A small diagnostic model used 92 signed movement-ability counts plus an intercept. Its coefficients were fitted only on the 4,096 training draws, using teacher probabilities transformed into score targets.

The learned component alone achieved validation teacher error **0.02100**. Keeping A's neural correction frozen and fitting this component to the remaining teacher score achieved **0.01140**, compared with **0.04365** for pure B and **0.00980** for A.

The second model uses no fixed material addition at inference: both contributions are learned. Its inherited correction still comes from material-based training, and this diagnostic has not been quantized, exported, or tournament-tested. It supports a learned global component as a next experiment, not immediate tournament admission.

The existing 384-wide feature transformer has roughly 91.6 million position-specific embedding weights. A small shared component may make global material relationships easier to learn than requiring those relationships to emerge entirely through square-specific embeddings.

### Other checks did not identify a simpler implementation failure

Independent NumPy integer inference closely reproduced the saved validation metrics. Rust replay of two frozen positions under A, B, and C produced six exact score matches, including residual-versus-total mode handling. This argues against a loading or score-format error as the main cause.

An earlier QAT discrepancy was roughly 15 score units. It is too small to directly account for the observed evaluation degradation, although its effect on conversion gradients was not isolated because the preparatory checkpoints predated the correction.

About 14.9% of B's and 15.4% of C's quantized embedding entries changed from the parent, versus 7.1% for A. The embedding was not simply frozen. Activation boundary occupancy did not uniquely distinguish failures from successful models.

Limited output range is also insufficient as an explanation. On validation positions with absolute old material below 1,000, teacher error was 0.01645 for A, 0.05593 for B, and 0.05942 for C.

## Required promotion input fix

**Priority: required representation repair, independently of a pure NNUE retry.** The implementation should expose remaining promotion possibility, rather than relying only on current movement or a generic “has promoted” bit.

- [ ] Encode whether a piece can still promote under the actual rules and the identity of its promotion result. Preserve relevant base-piece provenance where it changes promotion or movement.

- [ ] Distinguish native Free King from already-promoted Free King at the same square, for both colors and both evaluator perspectives.

- [ ] Audit other pieces with identical movement encodings for different promotion destinations or remaining rights. Determine whether explicit piece identity is also needed beyond promotion information.

- [ ] Distinguish future promotion eligibility from whether promotion is legal on a particular move. Use rule-aware eligibility first; evaluate move- or zone-dependent features separately.

- [ ] Version the input schema and its hash. Define checkpoint migration explicitly; do not silently reinterpret existing feature rows.

- [ ] Add focused regression tests for promotion, captures, accumulator updates, and export/inference parity. A migration intended to preserve the old function must demonstrate that behavior before learning begins.

Acceptance means that boards differing only in meaningful promotion rights no longer have indistinguishable NNUE representations, inference agrees with training, and old checkpoints are either migrated deliberately or rejected with a clear schema mismatch. Actual promotion decisions still require playing-strength validation.

## Experiments to try next

Run the experiments in stages so that representation repair, global material learning, and end-to-end optimization are not all changed without controls. Start with a 384-wide pilot; expand promising methods to both 384 and 512. Compare models within each width using matched data, grouped splits, and training exposure. Keep the existing teacher-selection standards.

### First establish a control with the corrected inputs

Train a material-retaining A-style model using the revised input schema. Compare it with the corresponding existing A model. This isolates whether the representation and migration work before removing material.

Use the new control as the reference for subsequent conversion checks. The original parent and an already-degraded conversion checkpoint are insufficient acceptance baselines.

### Then learn the global value component

Repeat the successful diagnostic with a trainable global piece-value component that can use piece identity and promotion rights, alongside shared movement features. Initially freeze the positional correction and learn the replacement from corrected-rules teacher targets.

Compare teacher error, calibration, legal capture/trade behavior, promotion decisions, and sensitivity across material and game-phase strata. Export and evaluate the quantized candidate before drawing conclusions from a floating-point fit.

A fully learned sum of global and positional terms remains a learned evaluator; removing handcrafted material does not require eliminating useful architectural structure.

### Fine tune jointly after the replacement works

Unfreeze the positional network once the learned global component transfers well to held-out groups. Compare joint fine-tuning with keeping the correction frozen. Use separate learning rates if needed to avoid immediately disrupting the useful correction.

Make any fixed-material reduction conditional on held-out performance and diagnostic readiness, with checkpoint rollback on deterioration. Select tolerances in advance from baseline variation and the intended performance tradeoff. Do not repeat a fixed four-window removal schedule.

Temporary distillation from a strong parent can preserve useful behavior during conversion. Final supervision should allow corrected-rules search evidence to change inherited material prices, especially range-two-mover values.

### Test more integrated architectures only after a reliable learned baseline

If a single integrated network is still desirable, distill from the successful learned-component model and compare it with that reference. Factorized features, shared piece embeddings, regularization, larger effective batches, alternative losses, and learning-rate schedules are possible ablations.

Change one main factor at a time and use repeated seeds for promising candidates. The evidence so far does not support “train longer” or “replace BCE with Huber” as a sufficient repair: the failed models already fit training data well, and both objectives inherited the same unsuccessful conversion.

## Data and acceptance checks

Keep new data under corrected rules and use the established standards to choose teachers. Add legal capture, recapture, trade, promotion, and materially imbalanced examples, including sibling positions analyzed consistently. Cover pieces on varied squares and across opening, middle, and late phases, rather than obtaining more correlated copies of the same game patterns.

Sample range-two-mover situations as one explicit diagnostic stratum, without allowing them to dominate the corpus or treating their old prices as authoritative. Synthetic removals are checks, not unquestioned target labels.

- [ ] Preserve related starts, prefixes, siblings, and duplicates within the same split, and audit inherited checkpoint exposure.

- [ ] Use held-out groups for conversion acceptance and compare against the material-retaining control throughout.

- [ ] Inspect teacher error and calibration by phase, material imbalance, piece family, and promotion status, not only one aggregate.

- [ ] Add legal capture/trade and promotion suites to the synthetic sensitivity probes.

- [ ] Verify quantized export, score scale, output mode, accumulator updates, and inference parity.

- [ ] Use controlled playing tests before broad tournament admission.

- [ ] Keep final test results reserved for evaluation. If repeated experiments extensively tune to this validation set, establish an additional prospective development holdout without repurposing the final test set.

The promotion representation fix is the firm follow-up. The architecture and training changes above are proposed experiments whose benefits still need measurement.

## Evidence and reproducibility

Investigation date: November 9, 2026. Diagnostic engine and trainer source revision:
`ae59f527f512bff5830bf68447693e11b7301fa5`. The baseline parent was 384v4.5.
Individual candidate and baseline model hashes are recorded in the
[checkpoint diagnostic](../../benchmarks/v6_pure_20261109/diagnostic.json).
The frozen dataset manifest SHA-256 is
`7ea8b2e07e6db3aa11eef2f7181e9fa15b633dabb0264add4a7792866f541aa9`;
sampling seed is `20261109`. Test rows were excluded.

The diagnostics ran locally on Linux x86-64 with PyTorch and BLAS restricted
to one thread. They are evaluation-quality measurements, not runtime benchmarks.
The paired uncertainty intervals use 10,000 bootstrap resamples of the 52 related-game
validation groups. The learned linear component used teacher probabilities clipped
to [0.001, 0.999], transformed to scores with logit scale 2,822.9057434, and
least-squares coefficients fitted on training draws only.

The compact measurement records are committed with this plan:

- [Checkpoint and subset metrics](../../benchmarks/v6_pure_20261109/diagnostic.json)
- [Original conversion metrics](../../benchmarks/v6_pure_20261109/prelude-B-metrics.json)
- [Material sensitivity and learned-component probes](../../benchmarks/v6_pure_20261109/material-probes.json)
- [Results by material imbalance](../../benchmarks/v6_pure_20261109/material-buckets.json)
- [Paired group-bootstrap intervals](../../benchmarks/v6_pure_20261109/uncertainty.json)
- [Rust inference parity checks](../../benchmarks/v6_pure_20261109/rust-parity.json)

The source implementations at the investigated revision are
[NNUE features](https://github.com/yetiowin805/taikyoku_shogi/blob/ae59f527f512bff5830bf68447693e11b7301fa5/src/nnue/features.rs),
[material evaluation](https://github.com/yetiowin805/taikyoku_shogi/blob/ae59f527f512bff5830bf68447693e11b7301fa5/src/eval.rs),
and [v6 training and conversion](https://github.com/yetiowin805/taikyoku_shogi/blob/ae59f527f512bff5830bf68447693e11b7301fa5/training/nnue/v6_fit.py).

Full local diagnostic inputs and the investigation scripts remain in the
`v6-training-resume` worktree under `data/analysis/v6-pure-20261109/`,
`training/nnue/diagnose_v6_pure.py`, `training/nnue/probe_v6_material.py`,
and `examples/nnue_material_probe.rs`. Those scripts and large inputs are not
included in this documentation change. The frozen training snapshot is
`/mnt/HC_Volume_107032879/taikyoku/nnue-v6-training-20261107` on the VPS.

This repository document was adapted from the
[findings and retry plan](https://chatgpt.com/space/page_8dd0d81ff29481919270efe3047ea183).
It records an open promotion-input fix and proposed experiments; it does not
change production models, tournament entries, or training recipes.
