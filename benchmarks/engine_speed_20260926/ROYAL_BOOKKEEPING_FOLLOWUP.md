# Cached royal bookkeeping follow-up

Date: 2026-09-27. Baseline: `3da967e` with the global-attacker mask.

`Board` now keeps the count of royal pieces and the sole royal's square when
that count is one, for each color. Placement, replacement, capture, movement,
removal and cloning update the fields. A removal that leaves exactly one royal
finds its square from the remaining piece list; this rare update keeps common
search queries constant time. `GameState::has_lost` reads the count, and the
search's last-royal check gates read the cached square/count.

The board invariant test compares both cached values with a fresh scan after
many mutations, including moves and captures of royal pieces. Debug and release
library suites each passed 389 tests (4 ignored). Every warmup and measured
fixed-depth search matched the baseline's complete signature: chosen route,
score, depth, static evaluation, node/qnode counts and ordered root lines.

| Variant vs `3da967e` | Paired searches | Wall-time geometric mean |
| --- | ---: | ---: |
| Royal count only | 96 | 0.9957 |
| Count plus sole-royal square, screen | 96 | 0.9920 |
| Count plus sole-royal square, separate confirmation | 192 | 0.9879 |

All four agent aggregates improved in confirmation (0.9774, 0.9905, 0.9886,
0.9953); 10 of 12 position aggregates improved. The retained change saves
about 1.2% wall time on this fixed-depth corpus. This is a throughput result,
not a strength tournament.

The count-only, screen and confirmation measurements are recorded above. Use
this directory's `run.py` with the local corpus to repeat them.
