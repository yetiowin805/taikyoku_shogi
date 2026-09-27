# Board attack-scan prefilter, 2026-09-27

This exact change builds on the per-color occupancy masks at `d4847bb`.
Before running the existing per-piece attack test, `Board` now skips pieces
outside the conservative near-window/ray region unless their type is flagged
for global scan. `should_check_piece_for_target_position` and all specialized
attack checks still run for retained candidates in the original piece-list
order. Virtual-board attack checks keep the generic full-list path. The
victim-region mask is shared by board and search through `attack_utils`.

The final code passed the complete debug and release library suites (389
tests, four ignored) and all **48 warmups and 96 measured paired searches**
matched complete search signatures. Its candidate/baseline wall-time geometric
mean was **0.9800** (2.0% less). An earlier prototype with the helper in
`search.rs` matched another 96-pair screen (0.9772) and **192-pair
confirmation** (0.9746). The final code-layout check is the most directly
applicable timing; the other runs confirm the direction. All four agent
aggregates improved in the final check.

[Final plan](results/attack-filter-20260927/final-plan.json),
[final pairs](results/attack-filter-20260927/final-pairs.jsonl.gz), and
[final summary](results/attack-filter-20260927/final-summary.json) are saved
alongside the prototype screen and confirmation data.
