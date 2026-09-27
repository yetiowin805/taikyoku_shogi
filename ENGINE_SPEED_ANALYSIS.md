# Engine & search speed analysis

Findings from a code read of `board.rs`, `movement/`, `game_state.rs`, `attack_utils.rs`, `move_simulation.rs`, `eval.rs` and `search.rs` (main at `2598690`), plus a few small sanity measurements. The original analysis made no engine changes; subsequent implementation status and measured results are recorded below. Items are grouped as follows:

- **A. Fundamental engine changes.** Representation, move generation and attack detection. These help any algorithm (alpha-beta, MCTS, self-play data generation, featurization), and most can be verified as *exact* (identical nodes, scores and routes) with the existing `benchmarks/search_speed_20260920` harness.
- **D. Core engine deep dive** (placed after A). This covers game-state representation, legal move generation, path checking, make/unmake and attack detection at the function level, with per-piece-class measurements. It turns A1–A5 into concrete changes and adds new findings: duplicate two-step moves, blocked-piece cost, self-capture sweeps, and a Vice General rules bug.
- **B. Generic alpha-beta improvements.** Standard techniques from chess and shogi engines that apply to almost any αβ searcher. Some are exact; most change the tree and need strength testing.
- **C. Taikyoku-specific experiments.** These depend on this game's rules (804 pieces at the start, capturing-range sweeps, 36×36 board) or on this engine's selective search design.

Speed-up estimates are **order-of-magnitude guesses with the reasoning shown**. They are not measurements. Each one needs the paired, held-out protocol from `benchmarks/search_speed_20260920/README.md` before anyone claims it.

## B8 follow-up after PR #126

The conservative iterative-deepening soft stop is now enabled for timed
searches. After completing depth two or deeper, the engine skips the next
iteration when twice the last iteration's duration exceeds the clock left.
Untimed searches, including fixed-depth benchmarks, are unaffected. A fresh 72-case comparison
against merged `main` saved 20.3% summed clock at one second and 5.1% at
three seconds. Chosen moves matched in 48/48 and 23/24 cases respectively;
the differing three-second search stopped one depth shallower. This is a
tournament-throughput choice, not an established strength gain. See the
[soft-stop follow-up](benchmarks/engine_speed_20260926/SOFT_STOP_FOLLOWUP.md).

## Current pull-request result — 2026-09-27

This branch collects the retained speed changes since `main` at `198b2c0`.
It includes direct reach and irreversibility checks, lazy leaf-gate discovery,
candidate-region tables and board occupancy masks, Free Eagle and stage-B
shortcuts, cached royal bookkeeping, and the large-hang attack prefilter.
Two-step route deduplication also removes routes with the same destination,
promotion choice and progress-reset effect when the intermediate is empty;
routes that capture at the intermediate remain distinct.

The **final cumulative** fixed-depth comparison used 192 paired searches
across 12 positions and four agents. All 192 pairs chose the same complete
move, score and completed depth. Main-search nodes fell from 3,211,208 to
2,667,200 (16.9% less), and the paired wall-time geometric-mean ratio was
**0.6658 (33.4% less time)**. Individual trial percentages should not be
added together. The benchmark runners and per-change measurements are in
[the benchmark directory](benchmarks/engine_speed_20260926/README.md).

Under equal clocks, the branch completed an extra depth in 11 of 48
one-second searches and 4 of 24 three-second searches, with none shallower.
Chosen moves changed in four and two cases respectively. These are throughput
results on the local corpus, not an Elo or strength estimate. Debug and release
library suites passed 390 tests each (4 ignored), including board mutation and
capture/attack parity checks.

The two-step deduplication changes legal-route counts and search node counts,
so it still needs a strength tournament before merge. At merge time, record
the parent of the merge as a `kind: logic` history entry and run
`./deploy/freeze_history.sh`, as required by `AGENTS.md`. The sections below
record each intermediate experiment and rejected prototype. Generated plans,
logs and paired records are omitted from this PR. The retained comparisons can
be rerun with the benchmark scripts; discarded prototypes and their measured
conclusions are summarized here.


## Implementation details and experiment history — 2026-09-26 to 2026-09-27

### Initial candidate scan and progress labels

An exact search hot-path patch now caches the rare pieces requiring a global
attack-candidate scan, uses a fixed stack bitset for candidate indices, and
formats root/quiescence progress labels only when the periodic log fires.
Against `198b2c0`, the final patch matched all search signatures in 48 warmups
and 48 paired measurements (12 positions, four agents). Its geometric-mean
wall-time ratio was **0.9380** (6.2% less time); all four agents and all 12
position aggregates improved in this pass. This is a short paired screen, not
a strength result. The full measurements and reproduction details are in
[the candidate follow-up](benchmarks/engine_speed_20260926/CANDIDATE_FOLLOWUP.md).

The first pass also explored two-step deduplication and B8 soft stopping.
Deduplication was added in the follow-up below. The soft-stop trace suggested
clock savings, but its live timed comparison was interrupted; that change is
not in this branch.

### D2.2 two-step route dedup follow-up, 2026-09-27

Two-step routes with an empty intermediate square are now coalesced only when
the destination, promotion choice **and progress-draw reset effect** match.
Intermediate captures remain distinct. A sparse-fixture test compares the
complete post-search state of every omitted route with a retained route.
In 96 paired fixed-depth corpus searches on top of the exact hot-path patch,
all chosen complete moves, scores and depths matched. Main nodes fell from
1,605,604 to 1,333,600 (−16.9%); the paired CPU-time geometric mean was
0.9354 (6.5% less). Legal-move counts changed in 56/96 and node counts in
76/96 pairs, as expected. The full [deduplication report](benchmarks/engine_speed_20260926/DEDUP_FOLLOWUP.md)
contains the measured results and limitations. This changes move ordering and
still needs strength testing before merge.

### D2.5 Free Eagle reachability checks, 2026-09-27

The Free Eagle path builder no longer reruns full `can_reach` for each
constructed ordinary path or standard range target. It keeps the original
reachability check for in-place captures: a targeted parity test found that
one constructed return path is rejected by the old gate. The bounded change
matched all signatures in 48 warmups and 96 paired fixed-depth searches on
top of deduplication. Wall-time geometric mean was **0.9945**; position and
agent results were mixed, so the whole-search gain is inconclusive. See the
[Free Eagle follow-up](benchmarks/engine_speed_20260926/FREE_EAGLE_FOLLOWUP.md).

### D2.6 stage-B scan filter, 2026-09-27

Stage B now skips pieces whose movement configuration cannot produce a quiet
multi-leg move before running the boxed-in and capability checks. A table
parity test covers all declared types, colors, promotion states and base-type
variants. All 48 warmups and 96 paired fixed-depth searches matched complete
search signatures. Wall-time geometric mean was **0.9943**, with mixed
position/agent results; the whole-search gain is inconclusive. See the
[stage-B follow-up](benchmarks/engine_speed_20260926/STAGE_B_FOLLOWUP.md).

### Additional 2026-09-27 probes

- **Reuse the mover's movement configuration for dedup progress checks:**
  48 warmups and 96 measured pairs matched exact search signatures, but the
  wall-time ratio was **0.9990** with mixed results. The extra parameter flow
  was removed.
- **B8 conservative soft stop, actual timed trial:** after completing depth
  two or deeper, stop when twice that iteration's duration exceeds remaining
  clock. At 1 second (48 paired cases), every chosen move matched and summed
  clock fell 48.03 → 39.59 s; two cases stopped one depth earlier. At 3 seconds
  (24 paired cases), every chosen move matched and summed clock fell 72.02 →
  62.32 s; two cases were shallower. Scores changed in three total cases.
  This is a clock/strength tradeoff, so the prototype is **not enabled**.
  Trial details are in the
  [soft-stop report](benchmarks/engine_speed_20260926/SOFT_STOP_FOLLOWUP.md).
- **Unstable target sort (D2.3):** exact in both runs, but a 96-pair screen
  measured 1.6% less time and a 192-pair confirmation measured 0.4% more.
  The signal did not replicate, so the stable sort was retained.

### Precomputed attack-candidate squares, 2026-09-27

The near-window and distant-ray square lists used by
`generate_captures_hitting_square` are now built once for every victim square,
preserving the old scan order and attacker bitset order. This removes repeated
offset/bounds arithmetic in a major search hot path. After an exact 96-pair
screen, a separate **192-pair confirmation** matched all complete search
signatures and measured a **0.9517** wall-time geometric-mean ratio (4.8% less).
All four agents and all 12 position aggregates improved. The table holds
238,736 two-byte positions plus about 31 KB of slice metadata (~0.5 MB).
See the [candidate-square report](benchmarks/engine_speed_20260926/CANDIDATE_TABLE_FOLLOWUP.md).

### Per-color occupancy masks for attack candidates, 2026-09-27

`Board` now maintains one 1,296-square bitmask per color through placement,
removal, moves and cloning. `attacker_candidates` intersects that occupancy
with a precomputed near-window/ray mask, then maps only occupied squares to
piece-list indices. Board mutation tests validate the masks after replacements,
captures, moves and clones. The 96-pair screen and separate **192-pair
confirmation** both matched every complete search signature. Confirmation
wall-time geometric mean was **0.9404** (6.0% less than the candidate-square
list version), with every agent and position aggregate faster. See the
[occupancy-mask report](benchmarks/engine_speed_20260926/OCCUPANCY_FOLLOWUP.md).

### Board attack scan prefilter, 2026-09-27

`Board` attack checks now use the same conservative candidate region to skip
pieces that cannot pass the existing target-reach filter. Special global-scan
movers remain included; virtual-board attack checks retain their original
generic path. The final code passed full debug/release suites and matched
complete search signatures in a fresh **96-pair** run, with **0.9800**
wall-time ratio (2.0% less). The pre-refactor prototype also matched 96 and
192 pairs, with respective 0.9772 and 0.9746 ratios. The helper lives in
`attack_utils` so board and search share the same region mask. See the
[attack-filter report](benchmarks/engine_speed_20260926/ATTACK_FILTER_FOLLOWUP.md).

### Intermediate combined changes versus the original baseline

The intermediate branch at `7fed1be` was compared directly with the original
`198b2c0` baseline across **192 paired fixed-depth searches**. Every pair
returned the same complete chosen move, score and completed depth. Main nodes
fell 3,211,208 → 2,667,200 (−16.9%); the paired wall-time geometric mean was
**0.7781** (22.2% less). The individual gains do not multiply cleanly; this
is the measured net result on the local corpus. See the
[combined report](benchmarks/engine_speed_20260926/COMBINED_FOLLOWUP.md).

In a separate clock-matched trial, the final branch completed one additional
depth in 6/48 one-second cases and 3/24 three-second cases. The chosen move
changed in 2 and 1 cases respectively. Both variants used their full clock;
these are extra completed depths from higher throughput, not a strength/Elo
result. See the [combined report](benchmarks/engine_speed_20260926/COMBINED_FOLLOWUP.md)
for both measured trials.

### Follow-up attack and allocation trials, 2026-09-27

The next three exact prototypes were set aside after paired timing: direct
piece-slot candidate iteration for board attack scans (192 pairs, 0.9980
wall ratio), appending common capability landings into one target buffer
(96 pairs, 0.9990), and a 1.68-million-entry flat from/to history table
(192 pairs, 1.0004, with ~6.7 MB extra per search context).

**Retained: incremental global-attacker mask.** `Board` now keeps a second
per-color occupancy mask for the few pieces whose attack probes cannot be
bounded by the target's near window or rays. Both board attack checks and
`generate_captures_hitting_square` combine this mask with the precomputed
candidate region, then visit candidate pieces in their original list order.
The 96-pair screen and separate 192-pair confirmation matched complete
search signatures. Confirmation wall-time ratio was **0.8937** (10.6% less),
with all four agents and all 12 position aggregates faster. Debug and release
library suites each passed 389 tests (4 ignored), including board mutation
mask invariants. See `benchmarks/engine_speed_20260926/GLOBAL_ATTACKER_FOLLOWUP.md`.

**Retained: cached royal bookkeeping.** `Board` now maintains the royal count
and sole-royal square for each color. `has_lost` and the search's last-royal
check gates use the cached values. A count-only screen was exact but only
0.4% faster; the combined count-and-square version matched all complete
search signatures in 96-pair and separate 192-pair runs. Confirmation wall
ratio was **0.9879** (1.2% less) versus the global-attacker-mask branch, and
all four agents improved. Board mutation invariants and both debug/release
library suites passed. See
`benchmarks/engine_speed_20260926/ROYAL_BOOKKEEPING_FOLLOWUP.md`.

**Retained: large-hang attack prefilter.** Before generating captures onto a
large enemy at a quiet-parent leaf, ask the fast board attack query whether
any side-to-move piece attacks that square. Only destination captures matter
to this gate. A targeted game-state test checked that generated destination
captures imply a board attack. The 96-pair screen and separate 192-pair
confirmation matched complete search signatures; confirmation wall ratio
was **0.9566** (4.3% less) versus the royal-bookkeeping branch, with every
agent aggregate faster. See
`benchmarks/engine_speed_20260926/HANG_PREFILTER_FOLLOWUP.md`.

**Set aside:** a Stage-B multi-leg occupancy mask was exact but saved only
0.5% in 192 pairs while adding mutation and cloning work. Incremental rank,
file and diagonal line words passed board-mask and path-clear parity tests,
but their first 96-pair integrated search run was 0.3% slower. Caching
promotion reach in a type table was exact but effectively flat in 192 pairs
(0.9994 ratio).

Before the hang prefilter, the cumulative `a9b102a` branch versus original
`198b2c0` matched best complete move, score and depth in 192/192 fixed-depth
searches; node totals were 3,211,208 → 2,667,200 (−16.9%) and paired wall
ratio was **0.6934** (30.7% less).

With the hang prefilter included (`2b64287`), a fresh 192-pair cumulative
check against `198b2c0` again matched all complete chosen moves, scores and
depths; node totals remained 3,211,208 → 2,667,200 and paired wall ratio
improved to **0.6658** (33.4% less). At equal clocks, the new branch completed
an extra depth in 11/48 one-second and 4/24 three-second cases, with no
shallower cases; chosen moves changed in 4 and 2 cases respectively. These
clock-matched differences are throughput effects, not an Elo estimate.

This follow-up checks off four bounded parts of the proposals below:

- [x] **D3: limited-range path rescanning.** `generate_simple` visits each ray
  square once instead of checking every prefix again. The direct Simple reach
  query likewise stops rescanning already-checked squares. It preserves the original
  ordered raw output, including the duplicate enemy landing emitted when another
  in-bounds step remains. Removing that duplicate is a separate behavior review.
- [x] **D3: direct jump reach.** Check the requested offset and landing occupancy
  instead of generating a vector of every legal jump and searching it.
- [x] **D4: temporary reach/jump lists.** Directional-irreversibility checks query
  the needed reverse direction/offset directly and short-circuit. They preserve
  the existing color adjustment, promotion-specific configurations and progress
  counter semantics. This does not implement the larger compiled-spec proposal.
- [x] **D5.5: two-leg intermediate allocations.** Tengu/Peacock diagonal and Hook
  Mover orthogonal attack checks use two stack slots instead of a heap vector,
  preserving candidate order and duplicate suppression.
- [ ] The remaining proposals require separate implementation and validation;
  their estimates below remain the original analysis, not measured PR gains.

See [the follow-up validation record](benchmarks/engine_speed_20260926/README.md)
for parity tests, paired search measurements and reproduction instructions.

## Follow-up experiments and next steps — 2026-09-26

- [x] **A2/A7 (bounded part): lazy leaf entry checks.** Resolve last-royal evasions before any
  zero-budget return. Otherwise, test cheap sufficient capture-entry flags
  before tactical discovery, short-circuit remaining capture checks, and only
  discover loud promotions at entry when captures have not already opened q.
  Quiescence still generates its original ordered candidates with unchanged
  eligibility flags. This removes redundant discovery; it does not prune moves.
  The initial six-position/four-agent screen measured **9.9% less search CPU
  time** (equal-case geometric mean; 9.3% less summed CPU time), with exact
  scores, routes, root lines and node counts in all 48 measured pairs.
  A follow-up on six reserved positions saved **17.1% CPU time** (16.6% summed),
  again with exact parity in all 48 pairs and improvement for every agent and
  position. Debug and release library suites each passed 379 tests (4 ignored).
  These are two small samples, not a universal speedup guarantee.
- [ ] **A3/D3: blocker bitsets — next small prototype to finish.** Actual-set
  component membership queries were roughly 13 times faster for nonempty sets,
  but the integrated prototype changed Debug formatting in nested two-step
  NNUE feature names. Model loading correctly rejected it. Preserve the exact
  descriptor schema and verify existing checkpoints load before measuring
  whole searches. The prepared formatting fix is not yet tested; no integrated
  speedup is established.
- [ ] **A1/D3: line occupancy masks — promising, larger follow-up.** Synthetic
  ray-clear queries were 1.5–5.6 times faster. Prototype mask maintenance and
  make/unmake parity, including intermediate captures and special movers,
  before drawing any whole-engine conclusion. Current figures omit those costs.
- [ ] **A4/D1/D5: cached movement properties — low priority.** Despite cheaper
  isolated lookups, full searches saved only 0.4% CPU time in the same screen,
  with mixed per-agent results. All searches matched; the gain is inconclusive.
  Added config fields/invariants are hard to justify on speed alone at present.
- [ ] Lazy progress-label formatting, repetition-count maintenance and broader
  move-generation proposals remain unverified in this experiment. In particular,
  do not assume a progress-counter reset makes earlier repetition history
  irrelevant; that requires a separate rules/correctness proof.

Tests used two handcrafted and two NNUE agents, sequentially at low priority,
limited to approximately one quarter of one logical CPU. The timings measure
process CPU time under throttling, not normal-load wall time or playing strength.
Component factors cannot be compounded into whole-engine speedups. The larger
estimates elsewhere in this document remain hypotheses.

See [the experiment report](benchmarks/search_light_20260926/REPORT.md) for raw
results, exact tested patches, limitations and the follow-up acceptance record.

## Follow-up 3 — two-step dedup, lazy labels, ID time waste (2026-09-26, cloud session)

Base: `a3ae944`. Measured in a shared 4-vCPU cloud container with the seed
weights and a throwaway bench (`search_speed.rs` opening + pawn-push middlegame,
self-play plies 120/240, an "open" start position without pawns/front ranks, and
a "sparse" position keeping royals, two-movers, capturing-range pieces and every
third other piece). **Wall-time noise here is ±20–40% run to run**, so only node
counts and exact parity are reliable. Timings below are indicative at best.
Re-measure with the pinned-CPU harness before accepting any speed claim.

- [ ] **D2.2 two-step route dedup (measured in that session; not in this tree).** For two-step pieces,
  a route whose intermediate square is empty is dropped if the plain move or an
  earlier empty route already reaches the same `(to, promoted)`. Capturing
  intermediates are always kept. Stage B also checks against stage A's first-leg
  plain moves. Results:
  - Opening, pawn-push middlegame, open, ply 120, ply 240: **identical** move
    lists, nodes, q-nodes, scores and routes (no duplicates arise there).
  - Sparse (free Hook Mover / Tengu / Capricorn class pieces): legal moves
    **2604 → 1929 (−26%)**, d2 nodes **54,486 → 43,162 (−21%)**, q-nodes
    27,146 → 23,099, **same score and same chosen route**. Wall time 641 → 580 ms
    in the one paired run, which is inside the noise band.
  - Caveats: the node order changes wherever duplicates existed, so this is
    position-exact, not node-exact. Routes can differ in the progress-draw counter
    (a leg's directional irreversibility), and the first route is kept. Plain moves
    emitted twice by overlapping capabilities are left unchanged. Free Eagle paths
    are not deduplicated. Needs a tournament or paired-harness check on real
    middlegames with free two-movers, and a `kind: logic` freeze if merged.
  - Tests from that session: `open_two_step_routes_are_generated_once_per_result`
    (not in this tree). The release library suite ran 368 passed; the 13 failures
    are all `git show <old rev>` history lookups that cannot work in that shallow clone.
- [ ] **A7 lazy q progress label (measured in that session; not in this tree).** `quiesce` stored a
  formatted `String` (`move_label`) for every searched q-move, only for 3-second
  progress logs. The experiment stored the piece and squares and formatted only when a log
  line was printed. It was **exact** on all six positions (nodes, scores, routes).
  The bounded gain is ≈ q-nodes × ~150 ns ≈ 0.5–1% of search time, below this
  machine's noise, so no speedup is claimed.
- [x] **B8 measurement: half of each timed move is discarded.** Timed searches (1 s,
  depth cap 8, q2) on seven self-play positions (plies 0–240). Each ID iteration
  took **2.5–6.8× longer** than the previous one (median ≈3.5–4×). The time
  after the last completed iteration, which the search throws away, was **5–98%
  of the budget, 52% overall**. For example: ply 0 completed d3 at 430 ms, then
  spent 570 ms on an unfinished d4. Ply 80 completed only d1 at 25 ms and spent
  975 ms on d2. A "don't start iteration d+1 unless `elapsed × ~4 < budget`" rule
  would have returned the same moves in about half the clock on 6 of 7 samples.
  Using partial iterations (keep a root move that finished at depth d+1 and beat
  the previous best) is the other option. This is the cheapest remaining lever
  for tournament throughput. Not implemented, because it changes timed
  behaviour and needs a games-per-hour and strength check.
- [ ] Not attempted in follow-up 3: D2.1 blocked-piece skip, line-occupancy masks (A1),
  blocker-bitset NNUE follow-up (already merged in #121), time-management change
  (B8 above). D2.1 was measured in follow-up 4 and is in this tree.

## Follow-up 4 — deterministic profiling, candidate scan, soft stop (2026-09-26)

**Method.** Wall time on the cloud box is too noisy, so this round used
**instruction counts (valgrind cachegrind, `Ir`)**. They are deterministic and
only exclude cache and branch effects. Six cached positions (opening, pawn-push
middlegame, open, sparse, self-play plies 120/240), seed weights, fixed depth
(d3; d2 for open/sparse), q2 PathAware. `callgrind` gave function-level profiles.

**Where instructions go (callgrind, opening d3, at `a3ae944` + dedup):**
`generate_captures_hitting_square` was **75% inclusive**. Its per-piece filter
`should_check_piece_for_target_position` alone was **51% self time** (48% in
the pawn-push middlegame): **39.4 M calls** at ~71 instructions each, meaning
every victim square scans all ~400 own pieces. The callers are quiescence victim
generation (38%), `stm_has_large_hang_take` (32%, still, after lazy leaf checks)
and `stm_has_royal_capture` (9%). Full move generation is only ~4.5%. The filter
is cheap per call; the problem is how many times it is called. That explains
why the earlier "cached movement properties" screen was neutral.

**Results (Ir, millions; each row adds to the one above):**

| Change | opening | pawnmid | open | sparse | sp120 | sp240 | exact? |
|---|---:|---:|---:|---:|---:|---:|---|
| `a3ae944` baseline | 5752 | 10051 | 20182 | 9213 | 7109 | 6314 | — |
| + two-step dedup + lazy q label | 5724 | 9999 | 20002 | 8379 | 7097 | 6306 | position-exact (sparse nodes −21%) |
| + boxed-in piece skip (D2.1) | 5704 | 9943 | 19908 | 8385 | 7066 | 6292 | **exact** |
| + self-sweep ordering (D2.7) | 5532 | 9286 | 20133 | 8451 | 6916 | 6348 | nodes −2/−5/0/0/−2/+1%, same scores+moves |
| + **reverse candidate scan** | **3706** | **6515** | **16398** | **7914** | **5194** | **4346** | **exact** |
| cumulative vs baseline | **−36%** | **−35%** | **−19%** | **−14%** | **−27%** | **−31%** | |

The cumulative row stacks every experiment, including ones not merged here.
This tree has the boxed-in skip and the reverse candidate scan only.

- [x] **Reverse candidate scan (in this tree).** `generate_captures_hitting_square`
  first builds a bitset of piece-list indices that could pass the filter. These
  are own pieces in the short-reach window around the victim, own pieces anywhere
  on its 8 rays, and the few unaligned movers (Tengu/Peacock/Hook family, Lion
  Hawk, Cannon Soldier). It then runs the unchanged per-piece code on that set in
  index order. Same pieces, same order, so results are **node-exact on all six
  positions**. It gives −6% to −33% instructions on its own. After it, the next
  hotspot is building the candidate set itself (35% self): an 11×11–15×15 window,
  8 rays and a full-army pass for exotics. Next steps: fix the window at radius 5
  and move the single 7-square stepper into the exotic group, precompute the
  exotic flag per type, keep an incremental exotic list, then line-occupancy
  masks (A1). The same trick applies to `is_position_attacked_by_pieces` (~7%
  inclusive). Test: `candidate_attackers_include_every_piece_the_filter_accepts`.
- [x] **Boxed-in skip (D2.1, in this tree).** A step/slide-only piece whose first step in every
  direction is own-occupied or off-board has no moves, so it is skipped before
  config lookup. Exact, with a parity test over 150 random plies. Opening movegen
  wall time drops about 26 → 15 µs, but it saves only **0.2–0.6%** of search
  instructions: interior-node full generation is a small share of the search.
  Test: `boxed_in_shortcut_only_skips_pieces_without_moves`.
- [ ] **Self-sweep ordering (D2.7, measured; not in this tree).** Quiet moves by capturing-range pieces are
  ordered after other quiets, by the material of their own pieces swept. Mixed
  results (−3% to −7% on three positions, +1% on three); same scores and moves.
  This is a behaviour change with small payoff. Consider dropping it, or test it
  in tournaments before keeping it.
- [ ] **ID soft stop (B8), measured with `SearchConfig::id_soft_stop`, off by default; not in this tree.** Before
  starting depth d+1, predict its time as the last iteration × the observed
  growth (clamped to 2–8×), and skip it if that would pass the deadline.
  - At **1 s**: **same move 16/16**, **32% less clock used**, one sample stopped
    a depth shallower (same move).
  - At **3 s**: 28% less clock, but **2/16 moves differed** and 3/16 were
    shallower. The predictor is too aggressive when growth is erratic (d1→d2
    can grow 40×).

  Both runs had background valgrind load, but each pair ran under the same
  conditions. Suggested follow-up: a more conservative predictor (for example,
  use the smaller of the last two growth ratios), or use completed root moves
  from the partial iteration instead. Measure games per hour and strength.
- **Tests in that cloud session.** Release and debug library suites: **370 passed**, 4 ignored. The 13
  failures in each are `git show <old rev>` history tests that cannot work in a
  shallow clone. That session's new tests were
  `open_two_step_routes_are_generated_once_per_result` (not in this tree)
  and `boxed_in_shortcut_only_skips_pieces_without_moves` (in this tree).
  This tree also has `candidate_attackers_include_every_piece_the_filter_accepts`.
- **Caveats.** These are seed weights only; the NNUE agents and the handcrafted
  checkpoints in the pinned harness were not measured here. Instruction counts
  are not wall time (cache effects are excluded). Before merging, confirm with
  the `search_speed_20260920` / `engine_speed_20260926` harness on the reference
  corpus. The −14% to −36% cumulative row includes the unmerged dedup, label,
  and self-sweep experiments, so it is not the gain of this tree alone.

---

## 0. Where the time goes today (sanity measurements)

Environment: 4-vCPU cloud container, `cargo build --release` (thin LTO, no `target-cpu=native`), `EvalWeights::seed()`, `src/bin/search_speed.rs` positions. These are single runs and noisy, so read them as rough numbers.

| Measurement | Value |
|---|---|
| Opening d3/q2 (PathAware) | 22,052 nodes (17,477 q) in **606 ms**, ≈36k nodes/s |
| Pawn-push midgame d3/q2 | 42,129 nodes (35,166 q) in **896 ms**, ≈47k nodes/s |
| Full legal move gen, opening (`All`, 280 moves) | **≈30 µs**/call (≈100 ns per move, ≈85 ns per own piece) |
| `CapturesOnly` gen, opening | ≈26 µs (almost as expensive as full gen) |
| `is_position_attacked_by_color` (avg over all 1296 squares) | **≈3 µs**/query (scans up to ~400 attacker pieces) |
| Attack scans over the 20 "large" enemy pieces (≈ what the leaf hang gate does) | ≈44 µs |
| `generate_loud_promotions`, opening | ≈4.6 µs, returns **16 moves** (see C6) |
| `make_move_for_search` + `unmake` | ≈0.23 µs (already cheap) |
| `size_of::<Move>()` | 32 bytes, not `Copy` (holds a `Vec` for Free Eagle) |

`--features search-profile` bucket shares (opening/midgame, d3): gen 73–78%, attack 5–11%, order 6–17%, make 2–3%, eval ≈0%. The "gen" bucket includes `generate_captures_hitting_square`, which the leaf gates call. It is therefore **not** mostly full move generation.

**Key sanity experiment.** In `leaf_or_quiesce` I temporarily forced `hang_caps = false` and `royal_caps = false`, which skips `stm_has_large_hang_take` and `stm_has_royal_capture`. In both benchmark positions neither gate ever fired: node counts, q-node counts, scores and best moves stayed **identical**. Wall time fell to:

| Position | Stock | Gates skipped | Share of time spent in the two gates |
|---|---:|---:|---:|
| Opening d3 | 606 ms | 343 ms | **≈43%** |
| Midgame d3 | 896 ms | 586 ms | **≈35%** |

(The change was reverted. It changes behaviour whenever a gate *would* fire, so it is a measurement, not a proposal.)

Summary: search cost is dominated by **"who attacks square X?" queries** (leaf gates, hang pruning, ordering), all built on one scan of the whole opposing army, followed by full move generation at interior nodes. Evaluation (handcrafted, incremental) and make/unmake are already cheap. The main lever is to make attack queries cheap, so group A comes first.

---

## A. Fundamental engine changes (benefit any algorithm)

### A1. Reverse ("from the target") attack detection with line-occupancy bitboards — **highest value**

**Now.** `is_position_attacked_by_pieces` loops over every attacker (~400 pieces at the start), runs `should_check_piece_for_target_position`, then runs a per-piece reach test. `generate_captures_hitting_square` does the same and also materializes moves. Each query costs ~3 µs, and the leaf gates make one per large enemy piece (≈20) at **every** AB leaf.

**Proposal.** Answer the question from the target square outward, the way chess and shogi engines do (`attackers_to(sq)`):

1. Keep **line-occupancy words**. Every rank, file, diagonal and anti-diagonal on a 36×36 board has ≤36 squares, so each fits in one `u64`: `ranks[36]`, `files[36]`, `diag[71]`, `anti[71]`. A square change on make/unmake flips 4 bits. The nearest blocker in any of the 8 directions is then one `tzcnt`/`lzcnt` on a masked word, which turns every ray scan into O(1) per blocker.
2. For a target square, walk the 8 rays blocker by blocker. The first blocker attacks if it is an enemy with Range movement in the reverse direction. Pieces further along the ray matter only for jumping (`BlockingMode::Jump`) and capturing-range (`Capturing`) movers. Visit them by repeatedly clearing the lowest bit. The number of visits equals the number of pieces on the ray, not the ray length.
3. Short movers (Simple ≤ N, Jumping offsets): look up a precomputed per-square neighbourhood and check the piece types found against a per-type "can hit offset (df, dr)" table (303 types × 2 colours × a 7×7 or 9×9 window; a few hundred KB of `u8`s, or a bitset per type).
4. Exotic movers (Tengu / promoted Peacock, Hook-Mover family, Lion Hawk, Free Eagle, Cannon Soldier, conditional jumpers) are few. Keep per-colour lists of them (maintained incrementally, see A4) and call the existing specialised `tengu_attack::*` routines only for those.

**Where it pays.** Leaf hang and royal gates (≈35–43% measured), AB hang pruning (`capture_hangs_high_value_piece`), ordering hang checks (`move_order_score`), `stm_last_royal_in_check` (called up to 3× per node), `last_royal_evasions`, `lr_flight_penalty` (9 attack queries per eval when enabled), and quiescence victim generation.

**Estimate.** Chess and shogi engines answer `attackers_to` in tens of nanoseconds. Here it would be more like **0.1–0.3 µs** because of the exotic piece set, which is a **10–30×** faster query. If attack-type work is about 50–60% of search time (≈40% gates + a share of "order" + "attack"), cutting it by ~85% gives **≈1.7–2.2× nodes/s** with *identical* trees. This is exactly verifiable with the existing harness.

Precedent: H.G. Muller's HaChu (Chu/Dai/Tenjiku Shogi) and his published notes on large-variant engines rely on cheap attack information, and on "view distance"/neighbour tables that give the next occupied square in each direction, rather than on per-piece scans. The line-bitboard trick is the 64-bit analogue of chess rank attacks: 36 squares still fit in one machine word.

### A2. Cheap interim fix for the leaf gates (before A1 lands)

If A1 is too large a first step, most of the gate cost can be removed without changing behaviour:

- **Pre-filter victims with an early-exit `is attacked by STM at all?`** before calling `generate_captures_hitting_square`, which materializes every capture on the square. Most large pieces are not attacked at all.
- **Fuse the two gates.** `stm_has_large_hang_take` and `stm_has_royal_capture` both walk the enemy list and both call `generate_captures_hitting_square`. Do one pass with early exit.
- **Keep an incremental list of enemy "large" pieces and royals** (A4). The gate currently scans all ~400 enemies to find ~20 candidates.
- **Order the cheap booleans first.** `generate_loud_promotions` always runs, even when `loud_parent` already decides the outcome.

**Estimate.** 1.2–1.4× overall. Exact.

### A3. Table-driven, allocation-free move generation

**Now.** Each piece does the following:

- `MovementConfig::for_piece` goes through a `OnceLock` → `Vec<MovementCapability>` (heap, enum with `Box`ed two-step legs).
- Each capability returns a fresh `Vec<Position>`. These are then `append`ed, then `sort` + `dedup`ed.
- `Range` + `Capturing` checks `cannot_jump_over: HashSet<PieceType>` with a SipHash lookup **per path square**.
- `generate_simple` with `max_distance > 1` re-checks the whole path at every distance, which is O(d²).
- `Position::offset` does bounds checks at every step.

The allocation-free iterators from the 2026-09-20 study removed some of this, but capability landings still allocate.

**Proposal.**

- Compile movement configs into a flat, `Copy`, per-(type, colour) descriptor: direction masks already colour-adjusted, jump offset lists as `&'static [i16]` square deltas, and a `rank` integer instead of `HashSet` for "cannot jump over". Taikyoku's range-capture rule is rank-based, so it becomes an integer compare. For the few non-rank exceptions, a `[u64; 5]` bitset over 303 types does the job.
- Use a padded mailbox (e.g. 36 + 2×3 border → 42×42) with a sentinel `OFFBOARD`. Bounds checks go away, and a direction step becomes a constant `isize` delta.
- Emit straight into the caller's `Vec<Move>` (or a fixed per-ply stack). De-duplicate with a 1296-bit stamp (21 `u64`s, cleared by generation counter) instead of sort + dedup.
- With A1's line words, a Range slide becomes "tzcnt to the first blocker, emit the span".

**Estimate.** Chess mailbox generators run at 5–15 ns/move. Here it is ~100 ns/move, and many opening pieces are fully blocked yet still pay the setup cost. A **3–5× faster movegen** is realistic. Full-list generation is maybe 20–30% of time once A1/A2 remove the gate cost, so this is worth **≈1.15–1.3× overall**. It is also a direct win for anything else that calls movegen (MCTS rollouts, featurization, self-play data generation, the `mi` player). Exact, provided emission order is kept or intentionally re-baselined (ordering is by score with stable sort, so emission order only affects ties).

### A4. Incremental piece bookkeeping

Keep these in `Board` (or `GameState`), updated in make/unmake:

- Royal count and positions per colour. `get_winner()` / `has_lost()` currently scans the army at every AB and q node, and `color_has_last_royal_in_check` / `last_royal_of` scan again.
- Lists of large / loud-floor pieces, exotic-attack pieces (for A1.4), capturing-range pieces and possible loud promoters (`piece_might_loud_promote` currently scans the whole army at every leaf).
- Material per colour already lives in `EvalInc`. Extend it with the counters above.

**Estimate.** 3–8%. Exact.

### A5. Compact data layout and a `Copy` move type

- `Move` is 32 bytes and holds `MoveData::FreeEagle { path: Vec<Position> }`, so every searched move is `mv.clone()`d. `SearchUndo` also holds a `Vec` for removed pieces. Use a packed 8-byte `Copy` move (`from:u16, to:u16, mid:u16, flags:u16`). Free Eagle paths can be encoded as a small direction-code sequence (≤ 4 steps × 3 bits fits in the flags) or an index into a per-node side table. Removed pieces go into a per-search undo stack (`Vec` reused across plies).
- Store `squares` as `[u16; 1296]` piece-ids (2.6 KB, L1-resident) with piece data in a struct-of-arrays table, instead of `[Option<Piece>; 1296]` (10 KB).
- `killers`, `history` (`HashMap<u32,i32>` with SipHash, looked up for every move in every sort) and `sib_reps` should become flat arrays or `FxHashMap`. See B4 for history.

**Estimate.** 3–10% (mostly from the allocation and hashing removals). Exact.

### A6. Bound the repetition scan

`repetition_count()` scans the **whole game** `rep_history` at every node. Tournament games run to 1,000–2,000 plies. Only positions since the last irreversible move can repeat, which is exactly `turns_without_capture_or_promotion ≤ 100`. Also, only the same side to move can match, so step back by 2. That caps the scan at ≤50 compares.

**Estimate.** Negligible in the opening, 2–5% in long late-game positions. Exact.

### A7. Small per-node overheads

- `SearchContext::timed_out()` calls `Instant::now()` on **every** call whenever a deadline is set. The `nodes & 0xff` throttle only gates logging. It is called per node and per searched move. Check the clock every 256–1024 nodes.
- `ctx.q_label = move_label(...)` formats a `String` for every searched q-move, and root does the same per root move. This is only needed for progress logs, so format lazily when a log line is emitted.
- `move_resolves_last_royal_check` and hang confirmation use `simulate_move` + `VirtualBoard`. `VirtualBoard::get_pieces_by_color` **clones the whole army** per attack query and every `get_piece` walks the delta. A real `make_move_for_search` / `unmake` costs ~0.23 µs and would usually be cheaper, since the NNUE accumulator is already detached for rule-only probes.

**Estimate.** 2–5% combined. Exact.

### A8. Build and toolchain

- `RUSTFLAGS='-C target-cpu=native'` for all tournament and worker builds, not just NNUE. PGO via `cargo pgo` typically gives 5–15% on branchy engine code. `panic = "abort"` and `lto = "fat"` give 0–3%.
- Until allocations are removed (A3/A5), a faster global allocator such as `mimalloc` is a low-effort 3–8%.

---

## D. Core engine deep dive: state, move generation, path checking

This section goes function by function through `position.rs`, `board.rs`, `piece.rs`, `game_state.rs` (generation, `apply_move`, `execute_single_move`), `movement/generator.rs`, `movement/irreversible.rs`, `path_utils.rs`, `attack_utils.rs`, `tengu_attack.rs` and `move_simulation.rs`.

### D0. Measurements behind this section

These come from a throwaway release binary (since deleted). It timed `generate_legal_moves_for_pieces_mode(&[piece], All)` separately for every piece of the side to move, 300 reps each, grouped by movement class. The middlegame positions come from depth-1 self-play from the opening (seed weights) and are not real tournament positions. **Blocked** = pieces that generate zero moves. **Raw** = the sum of per-capability landing lists, before promotion variants and dedup.

**Opening** (402 pieces per side, 280 moves):

| Class | Pieces | Blocked | Moves | Time | ns/piece | Share |
|---|---:|---:|---:|---:|---:|---:|
| Range (NoJump/Jump rays) | 203 | 198 | 8 | 11.8 µs | 58 | **40%** |
| Simple (steps, incl. multi-square) | 172 | 136 | 44 | 8.5 µs | 49 | **29%** |
| Capturing range (GG, VG, …) | 8 | 0 | **228** | 8.1 µs | 1016 | 27% |
| Two-step | 9 | 9 | 0 | 0.6 µs | 69 | 2% |
| Jumping / cond. jump / Free Eagle | 10 | 10 | 0 | 0.8 µs | — | 3% |

**Self-play ply 120** (361 pieces, 423 moves): range 175 pieces with 147 blocked (37% of the time); simple 163 with 96 blocked (31%); capturing range 7 pieces producing 213 moves (21%); one mobile Free Eagle costing 2.4 µs (8%).

**Self-play ply 240** (316 pieces, 509 moves): range 148 with 106 blocked (37%); simple 146 with 63 blocked (34%); capturing range 225 moves (24%).

**Floor:** a naive loop that probes the 8 neighbours of every own piece (still bounds-checked, via `Option<Piece>`) takes **2.3–3.0 µs**. Real generation takes 27–30 µs, **about 10× above that floor**, even though most pieces cannot move.

**Attack queries** (`is_position_attacked_by_color`, averaged over all 1296 squares): a hit costs **1.0–1.2 µs** and a miss costs **2.0–2.6 µs**, because a miss must scan the whole army. Hang tests and "is my landing safe?" questions are mostly misses, so they pay the higher cost.

**Duplicate two-step moves.** One piece of each type on an open board (two kings, four enemy pawns), counting moves emitted by `generate_legal_moves` against distinct resulting positions (Zobrist after make):

| Piece | Moves emitted | Distinct positions | Duplication | Full gen |
|---|---:|---:|---:|---:|
| Hook Mover | 3653 | 1294 | **2.8×** | 54 µs |
| Capricorn | 3279 | 947 | **3.5×** | 38 µs |
| Tengu | 2356 | 663 | **3.6×** | 55 µs |
| Peacock | 2043 | 782 | 2.6× | 20 µs |
| Lion Hawk | 644 | 335 | 1.9× | 10 µs |
| Free Eagle | 164 | 138 | 1.2× | 13 µs |
| Great General (single-leg control) | 137 | 137 | 1.0× | 4 µs |

**Opening move composition:** of the 280 legal opening moves, **156 capture only the mover's own pieces** (median 7, max 9 own pieces removed per sweep). Another 80 capture at least one enemy piece, and just 44 are plain quiet moves.

**Config census** (all types and promoted variants): 498 movement configs, of which 338 have 2–4 capabilities. 268 `Simple` capabilities have `max_distance > 1`. 10 capturing-range capabilities carry a `cannot_jump_over` `HashSet` of 3–8 types.

### D1. Game-state representation

**Today.**

- `Position { file: u8, rank: u8 }`, converted with `to_index()` (a multiply) at every board access. Every step is a bounds-checked `Position::offset` returning `Option`.
- `Board` holds `squares: Vec<Option<Piece>>` (1296 × 8 B = 10 KB), `piece_slots: Box<[u16]>`, and one `Vec<Piece>` per colour. Each piece is stored twice (square + list) and carries its own `position`.
- Removal is `swap_remove`, so list order changes on every capture, promotion (implemented as remove + place) and unmake (re-push). Generation order, and therefore tie-breaking in search, is path-dependent. It is deterministic, but it rules out caching anything by list index.
- Every property question goes back to `MovementConfig::for_piece` (a `OnceLock` + a `Vec<MovementCapability>` with `Box`ed two-step legs and `HashSet`s) and re-scans the capabilities. There are about a dozen such helpers: `has_two_step` (per piece per generation), `uses_capturing` (per executed move), `piece_reach` (allocates a `Vec` of jumps on **every make**, for the progress-draw counter), `has_range_movement_in_direction` and `has_only_capturing_range_movement` (per piece per attack query), `max_rank_delta_toward_promo` (per piece per leaf), `first_leg_range_dirs` (per eval), and `promotes_to` / `must_promote_on_rank` (big `match`es per emitted move).

**Proposal: a "compiled piece spec" plus square-indexed state.**

1. **`Sq = u16` everywhere in hot code**, on a padded mailbox. The largest leap is 5 (conditional diagonal jumps), so use a border of 5: 46 × 46 = 2116 cells, with off-board cells holding a sentinel. Direction steps become constant `isize` deltas (`N = +46`, `NE = +47`, …) and bounds checks disappear. Keep `Position` only at API and notation boundaries. An alternative that keeps the 36 × 36 layout is a `ray_len[sq][dir]: u8` table (10 KB).
2. **Board cells as `u16` piece ids** (0 = empty, sentinel = off-board). Put the piece table in struct-of-arrays form (`spec_id: [u16; N]`, `sq: [u16; N]`, `alive` bitsets per colour, iterated with `tzcnt`). The board then fits in about 4 KB of L1, pieces keep **stable ids** across make/unmake, and a promotion only changes `spec_id`. Stable ids are what make C1 (per-piece move caches) and incremental attack maps possible.
3. **One `PieceSpec` per movement identity.** That is (type, promoted, base type) × colour, about 1000 entries including the Whale and Rain Dragon variants. Each is built once from `MovementConfig` and is `Copy`, flat and `'static`:
   - `step_dirs[8]: max_distance`, `range_dirs: u8`, `jump_range_dirs: u8`, `capture_range_dirs: u8`, and `jump_over_rank` (or a `[u64; 5]` type bitset) instead of the `HashSet`;
   - jump offsets as square deltas;
   - `two_step` and `free_eagle` descriptors;
   - flags: `has_two_step`, `uses_capturing_range`, `only_capturing_range`, `reach_class`, `is_royal`, `is_big`, `promotes_into_big`, the promoted spec id, must-promote ranks, value;
   - `reverse_dirs` and `reverse_jumps`, for `move_is_directionally_irreversible` without allocation;
   - a precomputed "could reach offset (df, dr)" bitset (71 × 71 bits ≈ 630 B per spec) for attack pre-filtering (D5).

   Every helper above becomes a field load. This also fixes `reach_class_of`, which currently returns `Always` for **any promoted piece** (`base_piece_type.is_some()`), so every promoted piece bypasses the cheap attack pre-filter.

**Estimate.** On its own (with the current algorithms), roughly 1.3–1.6× on generation and attack scans from removing bounds checks, `Option` unwrapping, config re-scans and SipHash lookups. It is the prerequisite for D2, D3 and D5 and for A1, which carry most of the gain. It can be exact if piece iteration order is kept identical during migration. One way is to keep the `swap_remove` order semantics in the id lists at first, then change the order deliberately in a separate re-baselined step.

### D2. Legal move generation

**D2.1 Skip blocked pieces in O(1).** In the opening, 353 of 402 pieces (88%) generate nothing, yet they account for about 65% of generation time (50–60 ns each for config lookup, capability loop, per-capability `Vec`, sort and dedup). At ply 240, 175 of 316 pieces (55%) are still blocked.

- Maintain, per square, an 8-bit **`open_nbr[sq]`** mask of which neighbours are empty, plus a per-colour `enemy_nbr` mask. Update the 8 neighbours of each changed square on make/unmake: 1–3 changed squares × 8 bytes.
- A piece whose moves are only steps and rays (no leaps, two-steps, jumps or capturing ranges) has no moves exactly when `(spec.adjacent_dirs & (open_nbr | enemy_nbr[them])) == 0`. That is one AND per piece.
- Better still (for C1): cache a per-piece "mobile" bit and only recheck pieces adjacent to changed squares.

**Estimate:** opening full generation drops from ~30 µs to ~8–12 µs, and the middlegame by 1.5–2×. Exact.

**D2.2 Stop emitting duplicate two-step moves (largest single finding in this section).**

`generate_legal_moves_for_pieces_mode` and `push_two_step_moves` emit one move per (intermediate, landing) pair. For range–range two-movers on an open board, **2.6–3.6× more moves are emitted than there are distinct resulting positions** (table in D0). If the intermediate square is empty, the route has no effect on the result.

- Search pays for every duplicate: make, recursion, unmake. At depth-1 children the duplicates are **not** caught by the TT, because `alphabeta` returns through `leaf_or_quiesce` *before* the TT probe when `depth == 0`. So each duplicate costs a full leaf, including the ~40–50 µs leaf gates measured in section 0.
- Generation pays too. A single free Tengu or Hook Mover costs 38–55 µs, more than the rest of the army combined.

Canonicalise instead:

- A quiet two-step, or one whose intermediate is empty, is identified by `(from, to, promo)`. Emit it once, with a fixed rule for which intermediate to record (for example the first in generation order), so TM1 output and replay stay deterministic.
- Keep distinct entries only when the intermediate captures (the result differs) or the move returns to origin ("igui"-style captures).
- Generate the second leg into a per-piece 1296-bit "seen" stamp rather than re-walking full rays from every intermediate. Better, compute the landing set as the union of rays from all intermediates with line words (A1): each second-leg ray then costs O(1).

**Estimate.** Positions where Hook Mover, Tengu, Capricorn or Peacock are free are exactly the tactical middlegames where search struggles. There, interior branching can drop by 2–3× and generation by 3–5×. The resulting positions are identical, so full-width scores are unchanged. The node order changes (LMR move indices shift), which makes this "position-exact" but not node-count-exact.

**D2.3 Remove per-piece allocation, sort and dedup.** Every capability returns a new `Vec<Position>`, which `generate_targets_filtered` appends, `sort`s and `dedup`s for **every piece**. Measured raw vs final counts are nearly equal: duplicates only occur when two capabilities of the same piece overlap. Emit directly into the move list, and use a stamp (a 1296-entry `u32` generation-counter array) only for the few specs whose capabilities overlap (a precomputed flag).

**Estimate:** 20–35% of per-mobile-piece cost. Exact, if emission order is preserved (sorting currently puts each piece's targets in square order; a stable merge can reproduce that for parity, or the order can be re-baselined once).

**D2.4 Make captures-only generation proportional to captures.** `CapturesOnly` (26 µs) costs almost as much as `All` (30 µs), because it walks the same rays and only filters what it emits. Better:

- For each enemy piece, ask "who attacks this square" (A1 reverse lookup) and emit those captures. That gives MVV order for free.
- Handle capturing-range sweeps separately: walk each capture-range piece's rays with line words and emit only rays that contain an enemy.

This matters for quiescence entry without `prev_to`, for B2's captures stage, and for `generate_loud_simple_takes` / hang gates.

**D2.5 Free Eagle generation is quadratic.** `generate_free_eagle_moves_unfiltered` calls `self.is_legal_move(piece.position, next)` for each candidate. That calls `can_reach` → `capability_reaches`, and for Free Eagle this means `generate_for_capability(..).contains(..)`: **the whole Free Eagle target set is regenerated per candidate**. Each candidate also clones its `path` `Vec`. One mobile Free Eagle cost 2.4 µs (8% of all generation at ply 120), and `apply_move` regenerates all Free Eagle moves again whenever it gets a non-path Free Eagle move. Candidates are legal by construction, so drop the check. Encode the path in the move as up to 4 direction codes (3 bits each) instead of a `Vec`. Exact.

**D2.6 Staging overhead.** Stage B (`QuietMultiLegOnly`) re-walks all ~400 pieces to find the few two-step or Free Eagle pieces (2.4 µs even when it emits nothing), and recomputes their first legs. Keep a per-colour list of multi-leg pieces (A4) and cache the first-leg landing set from stage A.

**D2.7 Classify self-capture sweeps at generation time.** Capturing-range pieces (Great General class, 7–8 per side) produce **44–81% of all legal moves** at the measured points (228/280 opening, 213/423 at ply 120, 225/509 at ply 240). In the opening, 156 of those sweeps capture only the mover's own pieces (median 7). Today they are:

- generated one square at a time (~1 µs per piece);
- classified as quiet by `move_captures_enemy`, so they fall into the quiet list and are ordered by history, not by their huge material loss;
- then searched (LMR reduces them, but each still costs make/unmake plus a child).

The generator already knows how many friendly and enemy pieces each ray has passed, so it can tag each move with `own_lost` and `enemy_taken` at zero extra cost. That enables:

- ordering self-destructive sweeps last;
- a behaviour-changing experiment (C-type): at non-root nodes, prune sweeps where `own_lost_value − enemy_taken_value` exceeds a margin, or push them to a stage C searched only if nothing else cuts.

**Estimate:** the ordering part is nearly free. The pruning part can cut opening branching roughly in half, but must be tested in tournaments.

### D3. Path checking

- **`generate_simple` is O(d²) per ray.** For `max_distance > 1` it calls `is_path_clear_for_boardlike(from, target)` at *every* distance, re-walking the path from the start each time, then walks it again (`path_positions`) to find the blocker. 268 Simple capabilities have `max_distance > 1`. `capability_reaches` for `Simple` has the same pattern, and it is on the attack-detection path for every short piece within range. Replace it with a single walk that stops at the first occupied square, emitting it if it is an enemy, like `generate_range` already does. Exact.
- **`is_path_clear_for_boardlike` / `is_diagonal_…` / `is_orthogonal_…`** step with `Position::new` (bounds check) and `board.is_empty` (bounds-checked `Vec::get` + `Option`) per square. On a `VirtualBoard` each square also walks the move delta. With A1's line-occupancy words, "is the open segment (a, b) empty" becomes **one masked test**: `(line[k] >> lo) & ((1 << len) - 1) == 0`. Callers include `tengu_attack::can_reach_via_diagonal_range`, the Hook Mover and Peacock attack probes, `royal_probes::clear_alignment_ray`, the two-mover alignment eval term, and the hang and path-clear classification in search.
- **`capability_reaches` for `Jumping`** calls `generate_jumping` (allocates a `Vec` of every jump target) and then `contains`. Compute the needed offset and check it directly. Better, test the "could reach (df, dr)" bit from D1.
- **`cannot_jump_over: HashSet<PieceType>`** is probed with SipHash on every occupied square of every capturing-range ray, in generation, reach tests and victim-hit generation. Use a rank compare, or a `[u64; 5]` bitset over the 303 type discriminants (O(1), no hashing).
- **`execute_single_move` capturing-path removal** walks `path_positions(from, to)` and calls `remove_piece` on each square. With line words, only the occupied squares in the segment need visiting (iterate set bits).

**Estimate:** path checks are spread across generation, reach and attack code, so there is no single share. Removing the quadratic walk and the per-square `Option` and bounds overhead is worth roughly 10–20% of generation and attack time today. With line words the check itself becomes almost free.

### D4. Make / unmake

`make_move_for_search` + `unmake` is about 0.23 µs, which is cheap relative to everything else, so this matters only after D1–D3 and A1 land. Remaining waste:

- `move_is_directionally_irreversible` → `piece_reach` allocates a `Vec` and walks the config on **every** make (twice for two-steps, per leg for Free Eagle paths). Use `spec.reverse_dirs` and `spec.reverse_jumps` (D1).
- `execute_single_move` re-scans capabilities for `uses_capturing`, reads `get_piece` 4–6 times per leg (including "verify the piece moved" checks that cannot fail on generator output), and implements promotion as remove + place. That reorders the piece list; with stable ids it becomes a field write.
- Two-step moves execute as two full single moves.
- `SearchUndo.removed` is a fresh `Vec` per capturing move. Use a search-owned undo stack.
- `GameState::clone` copies `move_history` including Free Eagle path `Vec`s (0.35 → 1.0 µs as a game grows from ply 0 to 240). This is not hot today, but the undo-by-clone patterns in probes and the GUI grow with game length.
- **Probable rules bug (correctness, not speed):** `execute_single_move` removes every piece between `from` and `to` whenever the mover has *any* capturing-range capability, regardless of which capability produced the move. The Vice General (capturing range diagonally plus a **2-square orthogonal jump**) is the only type the census flagged. I verified that `make_move` and `make_move_for_search` both **delete the piece jumped over** by a VG orthogonal 2-jump, whether it is an enemy or the VG's own piece. Meanwhile the generator and `move_captures_enemy` treat the move as a plain quiet jump. Unless your rule source says the VG's jump captures, removal should only apply to moves along a capturing-range direction. Per `AGENTS.md`, a fix that changes move choice would also get a history-freeze entry.

### D5. Attack detection internals (concrete steps toward A1)

Interim improvements that do not need line words:

1. **Replace the per-piece filter with one bit test.** `should_check_piece_for_target_position` currently calls up to three config-scanning helpers per attacker per query, and promoted pieces always take the slow `Always` path. Use D1's "could reach offset (df, dr)" bitset per spec (71 × 71 bits): unaligned range pieces, out-of-reach steppers and wrong-direction pieces are rejected in a couple of instructions.
2. **Split the attacker scan by reach.** Short-range pieces (most of the army) can only attack a square from within an 11 × 11 window. Scan the ≤ 120 board cells of that window, which are mostly empty in the middlegame, instead of all ~400 pieces. Scan only a separate per-colour list of long-range pieces (range, two-step, Free Eagle, cannon, capturing range) globally.
3. **Answer misses faster.** A miss currently scans the whole army (2–2.6 µs), and hang tests are dominated by misses. Steps 1 and 2 cut exactly this case.
4. **Stop using `VirtualBoard` for hot probes.** `VirtualBoard::get_pieces_by_color` clones the attacker `Vec` for every query, and every `get_piece` checks the delta list. Hang confirmation and royal-evasion filtering can use real `make_move_for_search` / `unmake` (0.23 µs) on the search state instead.
5. **`tengu_attack` helpers** are already analytic (they compute the two candidate intermediates), but they allocate `Vec`s for the 0–2 intermediates. Use a fixed `[Option<Sq>; 2]`.

**Estimate:** steps 1–4 give roughly 2–3× faster attack queries without new data structures, and therefore about 1.3–1.5× on current search (attack-type work is 50–60% of time, section 0). A1 remains the larger end state (10–30× per query).

---

## B. Generic alpha-beta improvements

Unless noted, these change the searched tree. They need a strength check (Swiss or knockout against the current seed) and, per `AGENTS.md`, a `kind: logic` history freeze if merged.

### B1. Keep the transposition table between moves of a game

Every `search()` starts from an empty TT, and the 20-09 study deliberately clears written slots. Chess and shogi engines keep the TT across moves with an age / generation field. The previous search has already explored most of the tree under the expected reply, so the next search starts with hash moves and bounds at depth d−2, which typically saves roughly one iteration.

- Blocker: the Zobrist key includes `draw_key(turns_without_capture_or_promotion)`, which changes every ply, so a position one move later can never hit. Remove the counter from the TT key. Only adjudicate the 100-move rule as a node-level check, which already happens: `is_draw_by_progress_rule()` runs before the probe. Or fold in the counter only when it is ≥ ~90.
- Keep clearing on a model or weights change (the key must not survive across evaluators).

**Estimate.** 1.2–1.5× time-to-depth *in games*. Zero in isolated-position benchmarks, so measure with game replays.

### B2. Staged move generation with TT move first

Interior nodes currently generate **and score** the entire stage-A list (~30 µs of generation plus ordering, which includes hang attack checks) before trying the TT move. In chess, a large share of cut nodes cut on the TT move or the first capture.

1. Validate the TT move's pseudo-legality cheaply (piece present, reach test) and search it **before any generation**.
2. Generate captures only, ideally from A1's reverse attacks per enemy piece, which gives MVV order for free. Search the good ones.
3. Killers (validate each).
4. Quiets, scored lazily (history only, no hang tests on quiets).
5. Stage B (quiet multi-leg), as today.

**Estimate.** 1.1–1.25× overall; more once A1 makes capture-only generation cheap. Nearly exact if the tie order is preserved.

### B3. Late-move pruning and a log-formula LMR, suited to b ≈ 300

The branching factor is ~280–340 (vs ~35 in chess, ~80–100 in shogi). Current LMR is `R = 1`, or 2 after move 12, with no move-count pruning. In Stockfish-family engines (and their shogi derivatives YaneuraOu and Apery), the largest node-count savings come from:

- **LMR** with `R ≈ c · ln(depth) · ln(moveIndex)`. With move indices in the hundreds this reaches R = 3–5 for late quiets.
- **Late-move (move-count) pruning** at depth ≤ 2–3 in non-PV nodes: skip quiets after `N(depth)` tries (e.g. 20 + 10·d²), unless they are killers, promotions, or "near the action" (C3).
- **History-based pruning** of quiets with strongly negative history at low depth.

**Estimate.** 2–4× fewer nodes at depth ≥ 3, and higher still at depth ≥ 5. This is the biggest *search* lever, but it changes play, so tune the constants by tournament.

### B4. History tables that fit the game

Replace `HashMap<u32,i32>` keyed by (from, to), a 1.68M-key space, with a **(piece type, colour, to)** "piece-to" history: 303 × 2 × 1296 `i16` ≈ 1.5 MB. Use Stockfish-style gravity updates. Optionally add continuation history keyed by the previous move's (piece, to). Faster lookups (array vs SipHash) and better ordering generalisation, since the same piece type reaching the same square is the useful signal.

**Estimate.** 3–8% from speed alone, plus better cutoffs.

### B5. Interior PVS

Only the root uses null-window scouting. At interior PV nodes, siblings after the first are searched with the full `(-β, -α)` window. Standard PVS searches them with `(-α-1, -α)` and re-searches on fail-high.

**Estimate.** 5–15% at depth ≥ 4, smaller at d3.

### B6. Static-eval-based pruning at the frontier

These are cheap because eval is incremental:

- **Reverse futility / static null move:** at depth ≤ 2–3 non-PV, if `eval − margin·depth ≥ β` and there is no pending loud tactic, return eval.
- **Futility pruning** of quiets at depth 1 when `eval + margin ≤ α`.
- **Razoring** at depth 1–2.
- **Lazy leaf gate (engine-specific variant):** in `leaf_or_quiesce`, compute stand-pat first. If it already fails high (≥ β) and `prev_to` is not a major enemy, a q search would return immediately anyway (aside from fail-soft value details), so skip the hang, royal and promo gates entirely. In null-window search roughly half the leaves fail high, so this halves gate cost even before A1.

Margins must be large: capturing-range sweeps swing thousands. **Estimate:** 1.2–1.6× (nodes), mostly at shallow depth. Not exact.

### B7. Null-move tuning

Null move is already very effective here (zugzwang is rare). Try adaptive `R = 3 + depth/4` (+1 if eval − β is large), and allow null at depth 1 with an eval-only leaf. **Estimate:** 1.1–1.3× at depth ≥ 4.

### B8. Time management: stop wasting the incomplete iteration

`search()` returns the **last completed** iteration and discards the incomplete one, apart from a partial best when no iteration finished. With 1 s per move and each ID iteration taking several times longer than the last, a large fraction of every move's budget (often 30–60%) goes into an iteration that is thrown away.

- **Do not start an iteration that cannot finish.** Stop if `elapsed > budget / k`, where `k` is the observed time ratio between consecutive iterations (≈3–5 here). This gives the same move and saves the time, directly raising tournament games per hour.
- **Or use the partial iteration**, as chess engines do. Root moves are searched best-first, so once the first (previous best) move is fully searched at the new depth, any later root move that completed with a better score is a valid, deeper result.
- Add a soft/hard limit split, and extend time when the best move changes or the score drops.

**Estimate.** 1.3–1.7× more useful search per second of clock, or the same strength at lower `--time-ms`, which means more games per hour. Minimal code.

### B9. Aspiration and TT hygiene

- The root aspiration width is fixed at 500. Make it proportional to recent score volatility, and widen gradually on fail.
- Pack TT entries (16–32 bit key check + move + score + depth + bound + age ≈ 10–16 bytes, 4 per 64-byte bucket). Store the static eval so a TT hit can skip `evaluate`.
- The q-TT hit rate is **2–4%**. `q_tt_key` salts in `prev_to`, which fragments entries. Test sharing the main TT for q, or dropping the salt.

**Estimate.** 2–6%.

### B10. Parallel search: only for interactive play

Tournaments already run `--jobs $(nproc)` single-threaded games, so Lazy SMP would not improve throughput there. It is worth adding only for GUI or analysis play. Per-worker memory: 2^20 + 2^18 TT slots per concurrent search. Check L3 pressure when many workers each own a 40+ MB table.

---

## C. Taikyoku-specific experiments

### C1. Incremental per-piece move caches ("dirty-piece" generation)

Most of the ~800 pieces do not change their move set when one move is played. The set of pieces whose moves can change after a move is small and computable:

- Pieces with a ray through any changed square (found with A1's line words, one lookup per direction per changed square).
- Short movers and jumpers within their reach window of a changed square.
- Exotic two-step movers whose first leg crosses a changed square.

Cache each piece's target list, mark only these as dirty, and regenerate them. Undo restores the cached lists (store per-ply deltas). This is how some large-variant engines keep movegen cheap. The same machinery can maintain the attack maps incrementally (per-square attacker counts by colour), which makes hang tests, `attackers_to`, SEE and mobility terms O(1).

**Estimate.** Full-list movegen → 5–15% of its current cost. **1.2–1.4× overall** after A1/A3, with the largest gains in the blocked-in opening/middlegame. Exact, but complex. Do A1/A3 first, then decide whether this is still worthwhile.

### C2. Cheap full SEE

With attackers-to (A1) or incremental attack maps (C1), a real static exchange evaluation becomes affordable. It can replace several heuristics:

- `capture_hangs_high_value_piece` (net < 0.8·mover && landing attacked)
- the q "live delta" net-gain futility
- hang-aware MVV-LVA ordering

The README already lists "Full SEE" as not implemented. For capturing-range sweeps, SEE only needs the dest square plus the removed path pieces. Likely a small speed gain (fewer bad captures searched) plus a strength gain.

### C3. Locality ("relevance zone") pruning of quiets

On a 36×36 board most quiets are far from any interaction. At depth ≤ 2 non-PV, only quiets that meet at least one of these are searched in full:

- the move lands within distance k of the last move's from/to/path squares, an enemy royal, or an own piece under attack;
- the mover is large or a two-mover;
- the move is a killer or has good history.

Everything else is LMR'd hard or pruned. This is a game-specific form of B3, cutting the frontier b from ~300 to perhaps 40–80. **Estimate:** 2–4× at depth ≥ 3. Risky, so gate it on "no last-royal danger" and test by tournament.

### C4. Prune suicidal loud promotions

In the **start position**, `generate_loud_promotions` returns **16 moves**: the two Flying Generals jumping quietly to empty squares at ranks 25–32 inside the enemy camp and promoting. `leaf_or_quiesce` enters (promo-only) quiescence whenever this list is non-empty, so essentially every quiet Black leaf opens q and expands these moves, which are almost always hung.

- Apply the same hang test the AB path uses for high-value captures. For example: skip a loud promotion if the landing is attacked and `promoted_value − mover_value < attacker-side loss`. Or only open promo-q if at least one loud promotion lands on an unattacked square.
- At minimum, skip them for the `include_caps == false` entry.

**Estimate.** Potentially large in openings, where q-nodes (17k) almost match main nodes (22k). Perhaps 1.2–1.5× there. Changes behaviour.

### C5. Eval-side incremental terms and lazy NNUE

- `two_mover_mobility_of` (on in `models/ab-seed.json`: `two_mover_mob_k = 50`) regenerates first-leg landings for each range two-mover at every eval. With line words the landing count is `tzcnt` per direction: O(8) per piece, no allocation.
- `lr_flight_penalty` does 9 full attack scans per side per eval when enabled. That becomes cheap after A1.
- NNUE agents: `evaluate` = material + residual. If the residual is bounded (it is clamped), skip `acc.residual()` at q nodes whenever `material ± max_residual` is already outside `(α, β)`: **lazy evaluation**. For width 2048, where NNUE inference dominates (the 20-09 study found NNUE agents gain less from search-side work), this could save 10–30% of eval time. A smaller production width is the other lever. Measure Elo per second, not per node.

### C6. Root width management

Every ID iteration scores almost all 280–340 root moves, and unlimited searches deliberately do not narrow the root. Under a clock, consider:

- Stronger root LMR for moves that failed low by a wide margin in the previous iteration.
- **Multi-cut / ProbCut** at shallow depth for obviously bad root captures.
- Carrying root move scores across *game moves*: with B1, the previous search's scores for the expected position order the next root.

### C7. Opening reuse across tournament games

Tournaments replay a finite set of start positions (`data/raw/starts`) many times. A shared on-disk cache of root results (or a small opening book) for the first N plies per start position removes repeated work. This matters most for throughput at short time controls.

### C8. Alternatives to αβ (for completeness)

The NNUE pipeline makes a policy + value network plausible, and PUCT/MCTS (as in dlshogi) copes with huge branching factors by sampling. It would benefit from A1/A3/C1 as much as αβ does, but it is a research project, not a speed-up. Treat it as a separate track.

---

## Suggested order and combined expectation

| Phase | Items | Kind | Rough gain | Risk |
|---|---|---|---|---|
| 0 | D4 Vice General jump check (correctness) | rules | — | Confirm rule source first |
| 1 | A2, A6, A7, B8 (don't start doomed iterations), D2.5 Free Eagle legality, D3 quadratic `Simple` walk + `HashSet` → bitset | exact / clock-only | 1.3–1.6× nodes/s + 1.3× effective time | Low |
| 2 | D1 compiled `PieceSpec` + `u16` squares, D2.1 blocked-piece skip, D2.3 no per-piece alloc/sort, D5 interim attack filter | exact (order-preserving) | 1.5–2× on top | Medium (large refactor; parity-test every spec) |
| 3 | A1 line words + reverse attacks, A4, A5, D2.4 capture-proportional gen | exact | 1.5–2× on top | Medium (exotic movers need parity tests) |
| 3b | D2.2 two-step dedup, D2.7 self-sweep tagging/ordering | position-exact, node order changes | 2–3× fewer interior nodes where two-movers are free | Low–medium |
| 4 | A3 remainder, B2 staged gen, B4 history, B5 PVS | exact / near-exact | 1.2–1.5× | Medium |
| 5 | B1 persistent TT, B3 LMP/LMR, B6 frontier pruning, C4 promo pruning, D2.7 self-sweep pruning | changes play | 2–4× effective depth-time | Needs tournaments |
| 6 | C1 incremental caches, C2 SEE, C3 locality pruning, C5 lazy NNUE | changes play / large | open-ended | Research |

Compounding phases 1–3 is plausibly **3–6× nodes/s with identical trees**. Phase 3b collapses duplicate positions on top of that, and phase 5 adds a similar factor in time-to-depth. The gains do not multiply cleanly: each fix shrinks the share of the others. Validate each step with the following:

- **Exact items:** the `search_speed_20260920` harness (identical routes, scores and node counts; held-out positions; paired timing). The parity approach in `src/parity.rs` (e.g. A1 vs the old per-piece scan over all 1296 squares × a corpus of game positions, and all exotic movers) should gate A1/A3/C1/D1/D2. For D1 in particular, a spec-vs-`MovementConfig` parity test over every (type, promoted, base, colour) and every square of a few corpus positions is cheap and catches most migration mistakes.
- **Behaviour-changing items:** fixed-time Swiss/knockout vs the current seed, one idea at a time (per `IDEAS_TO_TRY.md`), with a `kind: logic` history-freeze entry on merge (`AGENTS.md`).

## Reference engines and ideas

- **Stockfish** and its shogi ports **YaneuraOu** / **Apery**: staged move picker, log-formula LMR, move-count pruning, piece-to and continuation history, reverse futility, persistent TT with aging, time management with soft/hard limits.
- **H.G. Muller's HaChu** (Chu/Dai/Tenjiku Shogi) and his notes on large variants: attack-map / view-distance representations for boards with many sliders, jump-capturing generals (the Tenjiku analogue of capturing range), and lion-type double moves.
- **Chess rank-attack / line bitboards**: one machine word per line to find blockers in O(1). A 36-square line still fits a `u64`.
- **dlshogi / AlphaZero-style PUCT**: the non-αβ option for very wide games.
