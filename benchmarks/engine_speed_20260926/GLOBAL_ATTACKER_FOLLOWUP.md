# Incremental global-attacker mask follow-up

Date: 2026-09-27. Baseline: `9289d30` on `codex/engine-speed-experiments`.

`Board` already tracked occupied squares by color. It now also tracks the
squares occupied by pieces that `needs_global_scan` accepts regardless of
near-window or ray alignment. Placement, replacement, removal, movement and
cloning maintain that mask. The board attack query and capture-to-victim
candidate builder union this small set with the precomputed near/ray mask and
map candidate squares to piece-list slots. Slot-order iteration preserves the
old scan order, including early-return behavior.

The board invariant test checks both occupancy masks after replacements,
captures, moves, removals and clones, including Tengu positions. `cargo test
--lib --quiet` and `cargo test --release --lib --quiet` each passed 389 tests
(4 ignored). Complete fixed-depth search signatures matched in all warmup,
screen and confirmation cases: chosen routes, scores, depths, static evals,
node/qnode counts and ordered root lines.

| Trial | Paired searches | Candidate/baseline wall geometric mean | Interpretation |
| --- | ---: | ---: | --- |
| Screen | 96 | 0.8891 | 11.1% less wall time |
| Separate confirmation | 192 | 0.8937 | 10.6% less wall time |

All four agent aggregates and all 12 position aggregates improved in both
runs. The confirmation per-agent ratios were 0.8695, 0.8585, 0.8872 and
0.9635. The effect includes the cost of maintaining the mask during search.
These are fixed-depth timing and parity results on the local corpus, not a
strength tournament or a guarantee for other positions.

The screen and confirmation measurements are recorded above. Use this
directory's `run.py` with the local corpus to repeat them.
