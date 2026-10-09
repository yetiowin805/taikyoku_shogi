# Quiescence correctness smoke checks

This PR resolves recursive last-royal checks before stand pat or qdepth-zero
returns, and prevents q scores from crossing incompatible selective contexts.
The evasion path restores its caller's capture policy after entering AB search.
It avoids repeating the initial royal check already performed by the normal
leaf path. Interrupted q work is not stored as a completed table result.

The cache key includes the previous landing, remaining and initial q budgets,
capture eligibility, path-clear eligibility, preceding AB wipe, and consumed
royal-extension state. Search-wide configuration remains fixed within a search.
This addresses selective-context reuse; it does not claim to resolve all
history-sensitive transposition-table issues. The old `q_hash_prev_to` config
field remains accepted for compatibility, but landing identity is always used.

Six regression tests cover cache contamination plus valid reuse, context-key
separation, mate at zero depth and a narrow window, quiet/hanging evasions,
checking captures reaching a zero-budget q child, state restoration, and
cancellation. Existing material values, NNUE turn calibration, capture filters,
delta pruning, LMR, and time-management policy are unchanged.

## Reproduce the cost smoke test

Build `analyze_position` in release mode for main and this branch, using separate
Cargo target directories (especially for archives with old source timestamps).
Retain both binaries, finish all builds first, then run sequentially on one CPU:

```sh
python3 benchmarks/quiescence_fixes/run.py \
  --baseline /absolute/path/to/main-analyze_position \
  --fixed /absolute/path/to/fixed-analyze_position \
  --data-root /home/frank/taikyoku_shogi
```

The corpus reuses two development targets and four preselected controls from
the earlier slot 673 investigation, all with the same W512 v4.5 model and full
original game prefixes. It verifies game/model content hashes, discards inherited
AB/experiment overrides, and explicitly pins qdepth 2. Two shuffled repetitions
at depth 2 cover all six positions; two three-second trials on each target use
the ordinary analyzer CLI's predictive stopping (not the tournament Fischer
clock). Elapsed search time is taken from completed-iteration events; wall time
also includes model loading and replay. Incomplete depth is never treated as
completed fixed-depth work.

These are local cost and behavior checks, not an Elo test. Resolving checks
and rejecting invalid cached bounds can change both moves and explored nodes.

## Recorded results

Main `981afcc` versus fixed search `e9710d4`, Rust 1.92.0, standard release
profile (ThinLTO, one codegen unit), one pinned CPU, no concurrent builds.
`results.json` preserves all 32 runs and binary/data hashes.

- All 24 depth-2 trials completed. All six positions retained their selected
  full move routes. Total completed-search time was 1,100 ms on main versus
  1,233 ms with the fixes: **12.1% more time**. Nodes increased from 294,852 to
  302,340 (**2.5%**). These are two repetitions of six small searches, not a
  precise prediction of tournament throughput.
- Per-position median depth-2 times (main → fixed): target174 118.5 → 124 ms;
  target176 34 → 50 ms; early 123.5 → 130.5 ms; middle 41.5 → 59 ms;
  late 207.5 → 228 ms; long-history 25 → 25 ms.
- Both targets reached depth 3 in all three-second CLI trials. At 174, the
  selected Woodland Demon route changed from promoted to unpromoted; at 176,
  both builds selected the depth-3 Rook move. These fixes alone do not establish
  a solution to the original two tactical decisions. Completed depth-3 median
  time rose from 1,849.5 to 2,071 ms at 174 and 1,294 to 1,534.5 ms at 176.
- These changes are correctness fixes with a measured cost, not speedups.
  Future performance work can optimize recursive check handling independently.

Validation: `cargo test --offline -j 1 --lib -- --test-threads=1` passed all 429
library tests (4 pre-existing ignored). `cargo test --release --offline -j 1 --
--test-threads=1` passed the same library tests, all 3 Fischer-clock tests, and
all 5 rolling-process integration tests. The latter exposed a pre-existing
parity assertion comparing nondeterministic elapsed microseconds; the PR
excludes only `elapsed_us`, retaining iteration depths/completion flags and
all move/search data in the comparison.
