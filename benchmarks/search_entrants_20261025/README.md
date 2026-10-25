# W512 v3 search entrants — 25 October 2026

The user requested immediate admission of all eight proposed search variants, then a ninth QFIX arm through the existing tournament scheduler, without a preliminary strength-screening tournament. No strength gain is claimed. All arms use the exact network and material weights of registered `NNUE_W512_v3`; the base checkpoint is not rewritten.

Source baseline for the implementation: `3a2044085acb4c4c7828b1812b8052fe917b7b04`. Deployment provenance and admission receipts are recorded separately after publication. The current tournament executable predates main's recursive-q correctness fixes, so deployment must preserve that executable's baseline search semantics rather than silently changing all established entrants.

## Entrants

- `NNUE_W512_v3_H`: bounded positive/negative quiet history and capture history; ordering only. Tables cap at 65,536 keys each. Capture keys include mover type/color, victim type, destination, promotion and intermediate square. History persists through iterative deepening, not across turns. Root history is trained on cutoffs; interior negative updates cover searched alternatives in the same generation stage.
- `NNUE_W512_v3_L`: reduce one ply less for quiet moves that newly attack a royal or rescue an attacked major. An affected-ray/near-square filter bounds attack queries. It is selective, not a complete tactical detector; candidate eligibility stays unchanged.
- `NNUE_W512_v3_N`: attempt null pruning only at non-PV nodes with static evaluation at least beta, retaining existing guards. No extra verification search.
- `NNUE_W512_v3_S`: retain otherwise hard-pruned hanging high-value captures at root/PV, relying on existing hang ordering to demote them. Probe one ply shallower when possible and re-search full depth on alpha improvement. Interior non-PV rejection remains. This can still miss sacrifices; depth-one probes necessarily have no remaining main depth to reduce.
- `NNUE_W512_v3_T`: after a quiet move, enter existing qsearch when an affected-square query verifies a capture of a major or royal. The attacker need not be cheaper than the victim. Existing candidate filters/caps remain. This checks an available threat around affected squares, **not** a complete before/after attack delta; a previously available exchange can also qualify. Immediate royal captures were already protected by baseline.
- `NNUE_W512_v3_Q`: use configured q depth (two in this baseline) at scouts as well as PV leaves; q=0 remains off.
- `NNUE_W512_v3_HL`: H + L, without any additional pruning.
- `NNUE_W512_v3_ST`: S + T, without Q's deeper scout horizon.

The explicit checkpoint field is `search_defaults.experiment`. Missing means `baseline`; unknown names fail Rust checkpoint validation. Experiments are shared code paths, not copied search implementations. Remove losing policies after adjudication.

- `NNUE_W512_v3_QFIX`: added at the user’s follow-up request. On the pinned live baseline, enable recursive last-royal q evasions before stand pat/TT/q exhaustion, include landing, exact q budget, entry budget and eligibility flags in q TT keys, prevent zero-budget TT reuse and aborted final stores, and restore evasion-parent policy. Other eight arms retain the live legacy q behavior. On current main these fixes already apply, so `q_fix` is an explicit marker with the same q behavior as main’s baseline. Never infer live behavior from the checkpoint name without its recorded engine build.

## Live admission

Use the existing `deploy/training_admission.py` with `--search-variants admission.json`. This mode derives descriptors from the registered base, verifies the shared NNUE hash, preserves all other weights/settings, and submits all nine arms in one content-pinned coordinator request. It does not fabricate a completed training status. Existing brackets and running games continue unchanged. Normal scheduling, teacher selection and reversible retirement apply.

The active game engine must advertise every requested policy through `tournament-search-experiments`; an older engine that ignores unknown checkpoint fields is rejected before admission. Both the engine and analyzer validate all frozen descriptors. Retries retain the same durable task ID and verify the coordinator receipt against live entrants. Updated engine tooling also refuses rollback to a binary that cannot run an admitted policy.

## Validation and evidence

Local `cargo fmt`, all-target/all-feature Clippy with warnings denied, and the full Rust suite passed (459 passed, four ignored). Ruff format/check passed; 97 deployment tests passed. Tests cover checkpoint-policy propagation, unknown-policy rejection, bounded histories, tactical quiet rescue and non-rescue, new royal threats, verified capture entry, scout q budgets, sacrifice PV/non-PV eligibility, null PV guards, and legal root moves for all policies. Existing training admission retry tests remain in place. No Elo or timing improvement is inferred from these tests.

Mechanism backing: [Stockfish history ordering](https://github.com/official-stockfish/Stockfish/blob/dbe600b7438662e9ade60b31fa0625259dcd0077/src/movepick.cpp), [Stockfish search](https://github.com/official-stockfish/Stockfish/blob/dbe600b7438662e9ade60b31fa0625259dcd0077/src/search.cpp), and the [full local search survey](https://github.com/yetiowin805/taikyoku_shogi/pull/177). These sources motivate experiments; their tuning constants and strength claims are not transferred to this game.
