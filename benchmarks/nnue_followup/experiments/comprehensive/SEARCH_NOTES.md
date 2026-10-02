# Search-side follow-up experiments

All variants are independent changes against `903986e`. No retraining.

## Captures onto a square

Adds a 4096-slot direct-mapped cache of full generated `Move` routes. Entries above 128 moves are not stored. The key uses repetition key (draw progress does not affect legal captures), movement-origin salt (Whale variants), and target square. Cache hits clone full routes and stable-sort by the board's current piece-list index: make/unmake restores board occupancy but deliberately permutes the list. Without this step, memoization silently changes search ordering, ties, and reductions.

Focused test checks full route equality after a piece-list permutation, changing draw counter, and both promoted Whale origins. Broader corpus parity should decide if it belongs in production. Additional heap footprint is bounded by entry/move count; Free Eagle routes have short additional path allocations.

## Direct promotion-zone generation

The baseline already emits promoted variants without allocating unpromoted moves. The remaining opportunity is avoiding movement rays whose destination cannot enter the zone. The experiment restricts Simple and Range capabilities to N/NE/NW in relative coordinates when the start is outside the zone, uses unchanged blockers/range semantics, then filters destinations before sort/dedup. Starts inside the zone retain full generation; two-step movers retain the existing full-route fallback because intermediate squares enable promotion.

Focused parity test covers every type that promotes into a big piece, both colors, seven ranks around/outside/inside the zones and friendly/enemy blockers. Corpus timings are required: the saved work may be small after existing leaf-gate memoization.

## Staged TT-move picker (research)

Resolves the TT coordinate key via legal generation of only its mover and searches the chosen full route before generating/ordering the full army. A fail-high can avoid remaining generation. Quiet multi-leg moves remain in stage B. Removes only the exact route from the subsequent stage, because identical coordinates can encode different multi-leg captures.

This is behavior-changing: searching the first move before ordering the rest changes history seen by that ordering and modifies selective-search work. It is not an exact replacement. Compare fixed-depth wall time, selected move/score disagreements and equal-budget completed depths. No playing-strength claim follows from lower node counts or faster search. Keep research-only without game matches and the repository's required logic snapshot.

## Persistent main TT (research)

Keeps the 1M-entry main-search TT across consecutive positions in the same recorded game and model. Leaves q TT fresh because those bounds depend on q context. Benchmark resets all retained storage on a model change, game change, repeated/reversed/skipped position. Adds movement-origin salt to AB keys.

Contrary to the original proposal, removing the draw counter is not a prerequisite for reuse: the actual next position has the same draw counter as that node in the previous search, and the counter changes progress-draw outcomes. The prototype keeps it. Most d2 searches cannot exploit intermove AB reuse: a prior d2 search stores child d1 entries, while the next root does not probe a root entry and its children were only depth-0 leaves before. Use d3 or budgeted depth4.

This is intentionally not production-ready. Production needs an owned game/evaluator/config session rather than thread-local storage, history-aware repetition policy, and validation of stale-bound interactions with selective search. The prototype makes legal moves but legality does not establish bound soundness or strength. Fixed-depth/fixed-time results are preliminary. Prefer a move-order-only warm start if bounds cannot be safely scoped; that narrower variant remains a follow-up.

## Reproduction

Use the root's exact paired runner for the capture cache and promotion rays. Use `research_compare.py` for the two behavior-changing prototypes. Always run heavy work under `flock /tmp/nnue-comprehensive/cpu.lock` and build each variant after `cargo clean --offline --release -p taikyoku_shogi`: a shared target directory otherwise reuses stale normal-library/example artifacts across source-export directories. Record binary hashes.
