# V7 sensitivity pilot

This is a development experiment to check whether S2, S3, Ao, Cu and E produce
material effects before funding the full v7 matrix. It keeps fixed material and
the existing 256-wide input schema. Current-piece identity and promotion inputs
remain part of full v7; they are deliberately held constant in this pilot.

## Comparisons

One shared four-window A prelude starts from the current active 256-wide residual
leader, selected by the tournament's order-neutral ranking. Collection uses the
existing current-teacher policy, with each root family bound to one teacher epoch.
Each of these seven configurations then runs with seeds 20261112 and 20261113:

- A/S1: the shared control, 85% searched probability and 15% verified game result.
- A/S2: the same objective, with quiescence-settled positions.
- A/S3: the same objective, with additional legal alternatives and continuations.
- Ao/S1: 70% searched probability and 30% verified game result.
- C/S1: the existing confidence-weighted robust logit/Brier objective.
- Cu/S1: C with confidence set to one, retaining the same sampling hierarchy.
- E/S1: A with bounded, training-only situation reweighting after four ordinary windows.

Missing outcomes fall back to the searched probability. Counterfactual children
never inherit the played game's outcome. All models retain the same fixed material
baseline. Each window contains 65,536 weighted draws with effective batches of eight.

The shared prelude measures throughput. Before any treatment starts, the coordinator
freezes one common budget of 12, 10 or 8 windows, using four concurrent trainers and
a 25% allowance for validation, exports and adaptive audits. Learning rates halve
four and two windows before the selected endpoint. Models export at the matched
endpoint, not at individually selected validation peaks. Predictions at windows 4,
8 and the endpoint permit matched comparisons if a later stage is incomplete.

## Collection

Select at most 384 training and 96 fresh validation related-game groups, one game
per group, with four spaced representative and two challenge roots per game. Training uses
one random and one targeted challenge; validation challenges are random.
Inherited exposure from both old lineage manifests and the v6 corpus is checked.
Quarantined groups and the final test split are excluded. This validation subset
becomes development data and must not be reused as a final test for full v7.

The common frozen evidence supports three overlapping views:

- S1: 75% representative, 20% challenge, 5% teacher alternatives.
- S2: 25% raw representative anchors, 50% settled representative, 20% settled
  challenge, 5% settled teacher alternatives.
- S3: 50% representative, 10% challenge, 40% legal alternatives/continuations.

Weights are equalized by related group, game and phase within each category.
Unavailable categories redistribute their mass; actual masses are reported.
Identical feature rows retain all view memberships. Feature bytes are shared by
hard links rather than copied for each view.

Every root receives a fresh 10-second primary search. Up to 192 roots receive
30-second searches from primary and challenger; up to 32 receive 120-second
verification. All labels require completed root iterations, with mate handling
and confidence inherited from the v6 evidence preparation.

S2 uses an explicitly requested qsearch trace with a one-second budget and depth
six. The worker replays its moves legally. Aborted or horizon-truncated paths are
not settled leaves. Normal engine search does not enable tracing. Changed leaves
receive independent labels; an unchanged leaf retains its root identity.

S3 chooses one root per group, one unplayed teacher candidate and one unplayed
random legal candidate. Random choice first samples uniformly among the nonempty
capture, noncapture-promotion and remaining-move categories, then within that
category. Each arm can extend by three primary moves and a completed qsearch line.
Immediate children and settled endpoints receive independent labels.

Collection stops after eight elapsed hours, including lease waits and preparation.
Any family with unfinished mandatory creation, settlement or broad-label jobs is
excluded in full. Budget cancellations are explicitly recorded; actual failed
jobs still block preparation. Optional unfinished audits are not fabricated.

## Adaptive sampler E

Cells use phase, side-to-move material balance (within or outside ±5% of total
absolute material) and legal promotion availability. Unsupported promotion cells
merge first, then material cells, within phase. Each eligible cell needs at least
20 independent groups with raw confidence at least 0.5, including in its capped
training audit. Unsupported mass keeps its original weight.

A fixed, group-balanced training audit contains at most 2,048 positions. Error
starts at the shared prelude; after window four, teacher Brier error updates an
EMA with factor 0.2 once per window. Ratios to initial error use a 0.002 floor.
Multipliers stay between 0.5 and 2 and preserve total supported mass. Audit IDs,
EMA, initial error and multipliers survive checkpoint/resume. No validation or
test error drives the sampler.

## Diagnostics and interpretation

Training stops dispatching at hour 14. The remaining budget, up to hour 16, is
available for diagnostics on one leased CPU. All variants use the same held-out
position panel. Report teacher/outcome Brier deltas, paired group bootstrap
intervals, prediction changes, both seeds, and baseline seed variation. Additional
panels cover promotion, range-two movers, settled positions and alternatives.

Screen for mean absolute probability changes of 0.005, row p90 changes of 0.02,
or a predeclared slice with at least 20 groups and mean change of 0.005. These are
practical screening thresholds, not significance tests or Elo claims. First check
the actual data overlap, confidence dispersion, outcome target shift, adaptive
support and sampling total variation. Too few groups, missing matched windows or
an inactive sampler make the corresponding conclusion inconclusive.

Up to 16 common validation roots also receive two-second searches by each completed
first-seed model. Distinct choices receive ten-second child searches by a common
current teacher. This shallow panel is diagnostic, not playing-strength evidence.
Pilot models are never automatically admitted to the main tournament.

## Operation

`v7_pilot.py prepare` accepts the tournament run, source repository, release worker
and NNUE tool, Python environment, parent sample manifests, output path and code
revision. `v7_pilot.py run OUTPUT` performs collection, preparation, training and
diagnostics sequentially. Preparation must finish successfully before launching.
The Python executable must retain its virtual-environment path; resolving its
symlink selects the system interpreter and loses the environment's packages.
Preparation and startup check that NumPy and Torch are discoverable before
starting expensive stages. Actual imports and fitting run under CPU leases.
Use an immutable isolated deployment, six flexible game-boundary leases for
collection, four concurrent training jobs, and one leased CPU for diagnostics.
Preserve tournament workers 0 and 2. The pilot publishes bounded, expiring demand
through the tournament's existing reservation protocol and takes only assigned
CPU leases. It refuses to overwrite another live analyzer's reservation. Build
and validate remotely before launch.

Recommended service bounds are six CPU equivalents and 8 GiB memory, with BLAS and
Torch threads fixed to one per worker. A service failure must remain visible;
inspect `pilot-status.json`, stage logs and `training/blocked.json` before retrying.
The original deadlines survive restart. No stage is marked complete merely because
it was interrupted. `report.json` and `report.md` distinguish incomplete runs from
matched-budget results. These controls do not cap the Codex desktop application's
memory; local work must remain limited to bounded text operations.

### Two-hour recovery trial

The November 13 collection completed, but the first training attempt failed
before any optimizer step because preparation resolved the venv Python symlink.
The authorized recovery reuses that collection and its prepared views, preserves
the failed attempt's state, and starts a fresh two-hour budget: 100 minutes for
training and 20 minutes for diagnostics, with a two-hour service runtime cap.

With `pilot_short=true`, the shared four-window prelude uses 1,024 draws per
window. Its measured throughput determines one frozen draw count for all 14
treatment runs, in multiples of 512 up to 8,192. Every treatment retains eight
windows, allowing E four ordinary windows followed by adaptive sampling. The
calibration allows twice the prelude cost plus five minutes for overhead. If
even the minimum draw budget does not fit, the budget records that risk; missing
comparisons remain inconclusive. These shorter runs are sensitivity evidence,
not equivalent to the original 65,536-draw training schedule or strength tests.
