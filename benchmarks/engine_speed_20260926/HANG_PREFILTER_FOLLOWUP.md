# Large-hang destination attack prefilter

Date: 2026-09-27. Baseline: `a9b102a` (global-attacker and royal caches).

At quiet-parent leaves, `stm_has_large_hang_take` used to generate every
capture that could hit each large enemy, then keep only moves landing on that
enemy's square. It now first asks the board's conservative attack query
whether the side to move attacks that square. A miss skips capture generation.
The filter does not suppress path captures that merely pass through the square:
those already fail the gate's destination test.

A targeted 20-ply sampled game-state test checks the implication from every
generated destination capture to a positive board attack query. Debug and
release library suites each passed 390 tests (4 ignored). All warmup and
measured fixed-depth searches matched complete signatures: chosen route,
score, depth, static evaluation, nodes, qnodes and ordered root lines.

| Trial | Paired searches | Candidate/baseline wall geometric mean |
| --- | ---: | ---: |
| Screen | 96 | 0.9711 |
| Separate confirmation | 192 | 0.9566 |

Confirmation saves 4.3% wall time on this fixed-depth corpus. All four agent
aggregates improved (ratios 0.9621, 0.9353, 0.9618, 0.9677), as did 11 of
12 position aggregates. This is a throughput/parity result, not a strength
tournament.

Raw plans, summaries, stderr and compressed paired records are in
`results/hang-prefilter-20260927/{screen,confirm}/`.
