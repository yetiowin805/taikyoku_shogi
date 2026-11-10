# Corrected qsearch for new v6 search entrants

The live corrected-rules tournament deliberately uses a content-pinned production
source snapshot, rather than current main. Current main already applies F inside
qsearch; the production snapshot keeps the old opt-in policies reproducible.

Apply `v6-qfix-defaults.patch` to the source preserved at
`/opt/taikyoku-v6-admission-20261108/source`, then build the game engine and
`analyze_position`. The patch changes only `src/eval.rs`, `src/search.rs`, and
`src/search/experiments.rs`. Its production-only addition makes
`TacticalLmrQFix` satisfy `q_fix()` as well as `tactical_lmr()`; current main does
not need that predicate because its qsearch corrections are unconditional.

New `SearchDefaults` and `SearchConfig` explicitly select `q_fix`; deserializing
an old checkpoint with no experiment field still selects `baseline`. Existing
explicit policies and immutable checkpoint files retain their identities. The
new `tactical_lmr_q_fix` policy permits transferring L without also adding H.
NNUE export also writes `q_fix` for newly trained baseline models.

Publish through `engine_updates` only after validating old-model fixed-depth
parity and both helpers against the old and new descriptors. This extension adds
new explicit policies but does not change any existing tournament identity.
Running game children keep their pinned executable. Admission uses the running
coordinator transaction; it does not pause or restart games.

Exact selection inputs, network hashes, and the 48 entrant definitions are in
`benchmarks/v6_search_20261109`. Deployment receipts record the production source
manifest, compiler, executable hashes, and parity checks.
