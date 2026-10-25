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

- `NNUE_W512_v3_QFIX`: added at the user’s follow-up request. On the pinned live baseline, enable recursive last-royal q evasions before stand pat/TT/q exhaustion, include landing, exact q budget, entry budget and eligibility flags in q TT keys, prevent zero-budget TT reuse and aborted final stores, and restore evasion-parent policy. Other eight arms retain the live legacy q behavior. On current main these fixes already apply, so `q_fix` is an explicit marker with the same q behavior as main’s baseline. Never infer live behavior from the checkpoint name without its recorded engine build.

The explicit checkpoint field is `search_defaults.experiment`. Missing means `baseline`; unknown names fail Rust checkpoint validation. Experiments are shared code paths, not copied search implementations. Remove losing policies after adjudication.

## Live admission

Use the existing `deploy/training_admission.py` with `--search-variants admission.json`. This mode derives descriptors from the registered base, verifies the shared NNUE hash, preserves all other weights/settings, and submits all nine arms in one content-pinned coordinator request. It does not fabricate a completed training status. Existing brackets and running games continue unchanged. Normal scheduling, teacher selection and reversible retirement apply.

The active game engine must advertise every requested policy through `tournament-search-experiments`; an older engine that ignores unknown checkpoint fields is rejected before admission. Both the engine and analyzer validate all frozen descriptors. Retries retain the same durable task ID and verify the coordinator receipt against live entrants. Updated engine tooling also refuses rollback to a binary that cannot run an admitted policy.

## Validation and evidence

Local `cargo fmt`, all-target/all-feature Clippy with warnings denied, and the full Rust suite passed (459 passed, four ignored). Ruff format/check passed; 98 deployment tests passed. Tests cover checkpoint-policy propagation, unknown-policy rejection, bounded histories, tactical quiet rescue and non-rescue, new royal threats, verified capture entry, scout q budgets, sacrifice PV/non-PV eligibility, null PV guards, and legal root moves for all policies. Existing training admission retry tests remain in place. No Elo or timing improvement is inferred from these tests.

Mechanism backing: [Stockfish history ordering](https://github.com/official-stockfish/Stockfish/blob/dbe600b7438662e9ade60b31fa0625259dcd0077/src/movepick.cpp), [Stockfish search](https://github.com/official-stockfish/Stockfish/blob/dbe600b7438662e9ade60b31fa0625259dcd0077/src/search.cpp), and the [full local search survey](https://github.com/yetiowin805/taikyoku_shogi/pull/177). These sources motivate experiments; their tuning constants and strength claims are not transferred to this game.

## Deployment checks

The VPS candidate preserves the deployed `63de99c` engine plus the `c82824e` Fischer-clock patch. Its exact source is commit `dcb54bd31db8e5b1693ac9e39e4fab76e0421106` on local branch `codex/nnue-search-live`, archived on the VPS beside the binaries. [Build provenance](build-provenance.json) records compiler, build command and SHA-256 hashes. The legacy source retains formatter output. Its 435 Rust tests passed across the library, clock and rolling-process suites; the first run exposed an existing test that compared wall-clock microseconds across processes, so the already-existing main test correction was applied. Both the [initial output](validation/live-rust-initial.log) and [corrected rolling-suite output](validation/live-rolling-corrected.log) are retained.

[Raw VPS preflight results](preflight.json) show exact equality of completed depth, score, move and cumulative nodes at every iteration through depth three, at plies 1, 80, 200 and 500 of recorded game `slot4601-NNUE_W512_v3-vs-NNUE_W384_v3-a-white.json`. Final node counts were 34,456; 65,374; 274,500; and 53,860. All nine actual NNUE descriptors then completed depth-two searches at ply 80 and passed both executables' model validation. This is a bounded compatibility and execution check, not a strength or speed estimate, and does not prove equivalence on every position. No preliminary match tournament was run.

The first rolling update failed before publication because a pre-existing admitted model had a symlink-spelled binding while the updater resolved only the entrant path. The updater now resolves both sides and validates every matching content pin. A regression test also verifies that changing the model still blocks publication. The installed VPS updater received only the policy capability guard and this alias fix, with exact-content checks and backups; its other content was preserved.

Main implementation validation is retained in [Rust output](validation/main-rust.log), [Clippy output](validation/clippy.log), and [deployment test output](validation/deploy-python.log). PR CI passed Rust, Python and NNUE-Python on implementation commit `afc028d`; the later alias fix has its own local 98-test deployment run.

## Admission outcome

All nine entrants were admitted and verified eligible in the existing live tournament. [The deployment receipt](deployment.json) records task `01792962264414409978-admit-search-variants`, status `applied`, and the exact selected engine/analyzer hashes. The new engine build ID is `431ea433c145642f48346ca74e69c435320fbe1fc8e3e20ac6182308b58a6aaf`. Existing games kept their pinned old executable; no coordinator restart or scheduling-rule change was needed. The baseline checkpoint hash was checked unchanged before and after admission.

[Installed updater provenance](updater-install.json) records the backed-up original and final hashes. The source archive is on the VPS at `/opt/taikyoku_shogi/data/raw/tourney/royal-nnue-v3-32-20260925/analysis/search-entrants-20261025/build/search-entrants-live-source.tar.gz`. Final frozen descriptor identities, shared NNUE storage and coordinator continuity are recorded in [roster verification](roster-verification.json). Strength conclusions must wait for normal tournament results.

## Merge preparation on 25 October

Integrated main `8b229ed8a4130d10d1a799dc7e16215dab771ee5` into PR184. Both textual conflicts retained the experimental tactical filtering and main's explicit-weight move API; the new helper also uses the checked coordinate accessors. The combined source passes 477 Rust tests (four ignored), 120 deployment Python tests, formatting and all-target/all-feature Clippy with warnings denied. [Merge validation](merge-validation.json) pins source hashes and the [Rust](validation/merge-rust.log), [Python](validation/merge-deploy-python.log), [Clippy](validation/merge-clippy.log) and [history-freeze](validation/merge-history-freeze.log) outputs preserve the checks.

`LOGIC_PRE_SEARCH_EXPERIMENTS` freezes that main revision, including its executable and historical analyzer. Recheck the actual main parent before merging and refresh this entry if main has advanced. This preparation does not publish a different engine to the live tournament; the admission receipt and legacy-backport provenance above still describe the running experiment.
