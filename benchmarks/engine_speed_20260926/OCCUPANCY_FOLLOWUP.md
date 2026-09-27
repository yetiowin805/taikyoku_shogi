# Per-color occupancy masks for attack candidates, 2026-09-27

This exact change builds on the precomputed candidate-square lists at
`c39ee6c`. `Board` maintains a 1,296-square bitmask for each color in place,
remove, move and clone operations. Each victim's precomputed near-window/ray
region is also a bitmask. Attack-candidate construction intersects the region
with the attacker's occupied squares, then maps the set squares to piece-list
indices. The existing stack candidate set restores the original piece-list
iteration order. The masks add 336 bytes per `Board`; the static victim table
uses 1,296 × 21 × 8 = 217,728 bytes.

The board mutation test checks occupancy against every square after
replacements, captures, moves, removals and clones. The existing attacker
coverage test and full debug/release library suites passed: 389 tests, four
ignored. The release search harness matched complete signatures for all
48 warmups and 96 measured screen pairs, then all 48 warmups and **192 measured
confirmation pairs**. Complete signatures include route, score, main/q nodes,
depth, static evaluation and ordered root lines.

The screen's candidate/baseline wall-time geometric mean was **0.9397**.
Confirmation was **0.9404** (6.0% less), with every agent and all 12 position
aggregates faster. This is a pinned-CPU speed result, not a playing-strength
study.

The screen and confirmation measurements are recorded above. Use this
directory's `run.py` with the local search-speed corpus to reproduce them.
