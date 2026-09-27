# Free Eagle path reachability, 2026-09-27

This bounded D2.5 change builds on two-step deduplication at `46ea559`.
The Free Eagle generator constructs ordinary ray paths and standard range
targets with occupancy checks already performed. It now skips the expensive
`piece.can_reach` call for those candidates while retaining the turn and
friendly-destination checks. In-place capture patterns keep the old full
reachability gate. A focused test found a constructed return-to-origin path
that the old gate rejects, so removing that gate would change the move list.

The focused parity test runs both colors, four edge/interior origins and 64
blocker patterns; every emitted move must pass the original `is_legal_move`
predicate, and off-turn generation remains empty. In the existing 12-position,
four-agent paired corpus, all 48 warmups and **96 measured fixed-depth pairs**
matched complete search signatures: route, score, main/q nodes, depth, static
evaluation and ordered root lines. The wall-time geometric mean was **0.9945**
(about 0.5% less), with mixed position and agent results. Treat the overall
speed result as inconclusive. The shortcut is retained because it removes a
known quadratic check from constructed paths and preserves exact output.

The paired measurements are recorded above. The comparison used `run.py` and
the local search-speed corpus described in this directory's README.
