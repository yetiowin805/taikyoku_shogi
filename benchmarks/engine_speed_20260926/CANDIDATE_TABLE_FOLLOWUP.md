# Precomputed attack-candidate squares, 2026-09-27

This exact change builds on `abf48a0`. Each victim square now has a read-only
list containing the original 11×11 nearby window followed by the original
eight distant rays, in the same order. `attacker_candidates` walks that list
and uses its existing stack bitset to preserve piece-list order. The table is
built once on first use; it contains 238,736 `Position` values (477,472 bytes)
plus about 31 KB of slice metadata. No attack or move eligibility changes.

The existing `candidate_attackers_include_every_piece_the_filter_accepts`
test passed, as did the complete debug and release library suites: 389 tests,
four ignored. The existing 12-position/four-agent release harness matched
complete search signatures in every warmup and every measured pair. The
**96-pair screen** gave a candidate/baseline wall-time geometric mean of
**0.9558**; all agents and all position aggregates improved. A separate
**192-pair confirmation** gave **0.9517** (4.8% less wall time), again with
every agent and position aggregate faster. Search signatures include complete
chosen route, score, main/q nodes, depth, static evaluation and ordered root
lines. The measurements are a pinned-CPU speed screen, not a strength study.

[Screen plan](results/candidate-table-20260927/screen-plan.json),
[screen pairs](results/candidate-table-20260927/screen-pairs.jsonl.gz),
[confirmation plan](results/candidate-table-20260927/confirm-plan.json),
[confirmation pairs](results/candidate-table-20260927/confirm-pairs.jsonl.gz), and
[confirmation summary](results/candidate-table-20260927/confirm-summary.json)
are saved here. Use this directory's `run.py` with the local search-speed
corpus to reproduce.
