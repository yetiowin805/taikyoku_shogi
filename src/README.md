# Engine

Core library (`src/lib.rs`): board, movement, eval, search, training, HTTP session API.

| Module | Role |
|---|---|
| [`game_state.rs`](game_state.rs) / [`board.rs`](board.rs) | Position, make/unmake, legal moves |
| [`movement/`](movement/) | Per-type configs and generators |
| [`eval.rs`](eval.rs) | Capability material, PST, tropism, two-mover mobility |
| [`search.rs`](search.rs) | Alpha-beta + PathAware quiescence (details below) |
| [`notation.rs`](notation.rs) | TSFEN1 / TM1 |
| [`training/`](training/) | Texel, workers, tournaments — [`training/README.md`](training/README.md) |
| [`debug_tool.rs`](debug_tool.rs) | REPL used by `cargo run -- debug` |
| [`server.rs`](server.rs) / [`session_api.rs`](session_api.rs) | Local GUI backend |

Defaults for `ab` (seed checkpoint / `SearchConfig`): **depth 2**, **quiescence depth 2**, **PathAware** capture pruning, no time limit. Override with `--depth`, `--model`, env `TAIKYOKU_AB_DEPTH` / `TAIKYOKU_AB_TIME_MS` / `TAIKYOKU_AB_QDEPTH`.

Eval (capability material, rank PST, multi-royal bonuses) is separate from search. Royal **position** bonuses (Crown Prince / Drunken Elephant approach) were removed so the agent does not march royals into danger without king-safety awareness. Planned holes from knockout games: [`IDEAS_TO_TRY.md`](../IDEAS_TO_TRY.md).

## Coordinates

- Internal storage is **0-indexed** (files/ranks `0..=35`).
- Human-facing display often uses **1-indexed** values; shogi-style viewers may flip file/rank.
- **Black** advances toward **high** ranks; **White** toward **low** ranks.
- Promotion zone: Black ranks `25–35`, White ranks `0–10` (opponent’s last 11 ranks).

## TSFEN / TM1

Portable strings for sharing positions/moves. **JSON remains the on-disk canonical form** (`BoardPosition`, `GameRecordV2`); TSFEN/TM1 is the clipboard / protocol layer.

Coordinates are engine-native **0-based** `file,rank` (`0..=35`) — same as JSON. Shogi file/rank flip is display-only.

Piece ids are unique PascalCase `PieceType` names (`Pawn`, `GreatGeneral`, …), not board display symbols (those collide).

**TSFEN1** (one line):

```text
TSFEN1 <b|w> <draw> <n> <piece>…
# piece = B|W: [+]<PieceType>[<Base>] @ <file>,<rank>
# example: B:Pawn@5,2   W:+DragonKing<Rook>@18,30
```

Alias: `startpos` = opening setup.

**TM1** move tokens:

| Kind | Form |
|---|---|
| Standard | `12,3-15,6` or `12,3-15,6+` (promote) |
| Two-step | `12,3-13,4-15,6[+]` |
| Free Eagle | `12,3-13,4-14,5-15,6[+]` (from + path) |

```bash
cargo run --release -- position tsfen --start opening
cargo run --release -- position tsfen --start data/raw/starts/SOME.json
cargo run --release -- position load-tsfen 'TSFEN1 b 0 …' --out /tmp/pos.json
cargo run --release -- game tsfen-moves data/raw/games/SOME.json
```

UCI stub (`cargo run` with no subcommand) speaks `uci` / `isready` / `position startpos` / `position fen <TSFEN1…>` / `position … moves <TM1…>` / `go`. `go` returns the first legal move as `bestmove` in TM1 — not GUI-ready.

## Debug REPL

```bash
cargo run -- debug
```

- **Replay:** `load`, `forward`/`f`, `back`/`b`, `goto`/`g` (plies = MoveRecords; rebuild-from-start)
- **Inspect:** `board`, `pieces`, `piece`, `moves`, `check`, `attacked`, `status`/`info`
- **Edit:** `turn`, `place`, `remove`, `clear`, `reset` (edits snapshot a setup and branch)
- **Branch / agents:** `move …`, `suggest [mi|random|royal|ab]`, `play [mi|random|royal|ab]`, `save [file]`

Type `help` inside the REPL. Coordinates are shogi-style (file 1 = rightmost, rank 1 = top).

---

## Alpha-beta search (`ab`)

Implementation: [`search.rs`](search.rs). Ultimate Shogi has a huge branching factor (~hundreds of legal moves from the opening, many capturing-range “corridor” wipes). The search is **selective**: it tries to look deep enough on contested lines while refusing to expand most quiet / mop-up capture trees. Several of the wins below are **approximate** — they can skip moves that would be considered in a full-width search.

### Pipeline overview

1. **Iterative deepening** from depth 1 to the configured max; each iteration scores (almost) every legal root move.
2. Interior nodes use **alpha-beta** with a **transposition table**, **null-move pruning**, **late-move reductions**, and **staged move generation** (defer quiet multi-leg / Free Eagle until needed).
3. Captures that hang high-value pieces (e.g. Great General mopping a pawn onto a guarded square) are **skipped** in AB.
4. At depth 0, the engine does **not** always enter quiescence: only after a **loud** AB capture (enemy material ≥ loud floor ≈ **648**), a **loud promotion**, or a **lesser-valued SimpleTake of a hanging large piece**. Other quiets get **stand-pat eval**.
5. Quiescence is **capture-only**, default **PathAware**: thin generation onto the contested square, top-N fanout, PathClear only as dest-recapture, hang skips, and material futility.

### Iterative deepening and time control

**Mechanism.** `search()` loops `d = 1..=depth`. Each completed iteration reorders the root move list (best first). If `max_time_ms` is set and the deadline fires mid-iteration, the search aborts and returns the **last fully completed** iteration (or a partial best only if no iteration finished).

**Rationale.** Deeper iterations inherit better move ordering; timed play needs a graceful stop without corrupting the previous result.

**Speedup.** Not a prune by itself; it enables timed search and makes selective search usable under a clock.

**Can miss.** Under a time limit, moves that would only appear as best at an **incomplete** deeper iteration are discarded. Unlimited searches (`max_time_ms: None`) do **not** soft-abort or narrow the root: every legal root move is searched each ID depth (modulo hang-pruned captures).

### Transposition table (main search)

**Mechanism.** Zobrist-keyed table (`1<<20` entries) stores Exact / Lower / Upper bounds with remaining depth and a preferred move. On probe, matching bounds cut the node; otherwise the TT move is tried first.

**Rationale.** Huge transposition rate on a 36×36 board with repetitive capture sequences; TT cutoffs and ordering are the largest “free” savings that stay correct when bounds are sound.

**Speedup.** Not isolated in harness numbers; essential for multi-iteration ID.

**Can miss.** Essentially nothing legality-wise. Hash collisions and depth-preferred replace can occasionally drop a useful shallower Exact entry (standard TT caveats). Bound cutoffs assume consistent eval.

### Null-move pruning

**Mechanism.** At interior nodes with `depth ≥ 2` (and β not in a mate score band), the side to move “passes”: turn flips, AB capture context is cleared, and a reduced search (`depth − 1 − R`, `R = 2`) runs. If that still fails high (≥ β), the node is cut off. When the reduced depth hits 0, the null child uses **stand-pat eval**, not quiescence. Consecutive nulls are disallowed.

**Rationale.** Ultimate Shogi almost always has near-null quiet moves, so true zugzwang is rare. Null-move is what makes opening depth-3 searches interactive in release builds.

**Speedup.** Part of the selective package that keeps opening `d3/q2` on the order of hundreds of milliseconds rather than multi-second full-width trees.

**Can miss (approximate).**

- Rare **zugzwang** / “only move loses” positions.
- Tactics that would only appear if the null-move leaf ran **quiescence** (null→depth 0 intentionally skips q).
- Mate-band positions skip null (correct caution).

### Late move reduction (LMR)

**Mechanism.** Late quiet non-killers (interior) and late non-captures (root) are searched at reduced depth once move index is high enough (`≥ 3`, depth ≥ 2; reduction 1, or 2 for very late moves). If the reduced score fails high (> α), the move is **re-searched at full depth**. Captures and killer moves are never reduced. Root also uses PVS (null-window then research).

**Rationale.** After TT / captures / killers, most remaining quiets do not change the PV; cutting their depth is the main way to survive large root fanout at d3/d4.

**Speedup.** Bundled with null-move / non-PV q caps for deep ID interactivity; no isolated × factor in comments.

**Can miss (approximate).** Quiet **setup / tempo** moves that look mediocre at reduced depth and never trigger re-search (false fail-low). Captures are safe from this path.

### Staged move generation (quiet multi-leg deferred)

**Mechanism.** Interior AB first generates `WithoutQuietMultiLeg`: captures, ordinary quiets, and **capturing** multi-leg / Free Eagle. Only if that stage does **not** β-cutoff does it generate `QuietMultiLegOnly` (quiet two-step / quiet Free Eagle). If stage A is empty, stage B runs alone.

**Rationale.** Quiet Lion / Free Eagle trees are expensive to generate and rarely needed once a capture or simple quiet already cuts.

**Speedup.** Defers the heaviest quiet generators; capture-only gen is separately measured faster than full gen on the opening.

**Can miss (approximate).** When stage A **does** β-cutoff, quiet multi-leg / Free Eagle maneuvers are **never searched** at that node. A quiet Lion reposition that was actually best can be invisible if some earlier stage-A move already looked good enough to cut.

### AB hang prune (high-value hanging captures)

**Constants.** `HIGH_VALUE_HANGER = 400`, `HANG_NET_FRAC = 0.8`.

**Mechanism.** Skip a capture in AB (root and interior) when:

1. the mover’s material value is ≥ 400 (capturing-range generals and similar),
2. net path material `(enemy − own) < 0.8 × mover`, and
3. the landing square is attacked by the opponent.

Never skip a capture that takes an enemy **royal** (King / Crown Prince) on dest, intermediate, path, or Free Eagle route. Those are cheap in material (CP=8) so they look like hung-Hook junk, but they can end the game. A capture that takes the **last** remaining royal is an instant win: it sorts first in AB and q, and the root loop stops once one is scored.

`SimpleTake` uses a cheap pre-move landing attack. PathClear / MultiLeg: interior **confirm-on-prune** (pre-move attack first; only if that looks hanging, simulate post-fire); root goes straight to post-fire. Path loot is included in the net (a GG that path-clears a Hook then lands safely is not skipped).

**Rationale.** After capturing-range tariffs were raised, Great General–class pieces love to “mop” low pieces onto guarded squares. Expanding those lines dominates the tree and is almost always losing.

**Speedup.** Large practical win on midgame positions full of GG/BG corridors (not a single published ×; removes entire classes of root/AB children).

**Can miss (approximate).**

- **Sound sacrifices** where net < 0.8× mover but the position is winning (deflection, discovery, forcing lines).
- Pieces valued **below 400** are never hang-pruned this way, even if they hang badly.

This skip is about hanging **movers**. Quiet-leaf hang-q (below) is about hanging **victims**. Do not conflate them.

### Leaf quiescence gating (loud captures, loud promotions, hang SimpleTakes)

**Loud floor.** `seed_loud_capture_floor()` ≈ `max(450×0.6×2.4, 50×8) = 648` (capturing-range / jump tariffs in [`eval.rs`](eval.rs)).

**Mechanism.** At AB depth 0:

- If the parent AB move’s captured enemy material (or **loud-promotion material gain**) is ≥ the loud floor → enter quiescence with `prev_to` = the AB landing square.
- Else if the side to move has a legal promotion into a two-mover / range-capturer (e.g. FreeKing→GreatGeneral) → enter a **promo-only** quiescence (no full capture fanout). TreacherousFox→MountainCrane is **not** loud (ordinary range, not `is_big_piece`).
- Else if STM has a dest-take of a hanging large enemy (`stm_has_large_hang_take`) → enter capture q so that take is resolved. Ordinary SimpleTakes still need a cheaper attacker (equal GG trades do not open q). Dest MultiLeg / dest PathClear of a hanging range two-mover count for **any** attacker (A+B, default on), including hook-takes-hook. Corridor dest-beyond PathClear still does not.
- Otherwise → **stand-pat eval**, no quiescence.
- **Non-PV** (null-window) leaves use `qdepth = min(config, 1)`; PV / fail-high research uses the full configured q depth.
- Null-move children clear the loud-capture flag, so they do not gate into q via that path.
- Late-move reductions **skip** loud promotions (same as captures).

**Rationale.** Full capture quiescence after every quiet leaf exploded wall time (especially SimpleTake trees). Q should **resolve the exchange** after something loud already happened in AB, and must not miss promotions that create GG/Lion/HookMover-class pieces (~thousands of eval).

**Speedup.** Primary reason deep ID stays interactive: most leaves never touch q. PathAware then thins the leaves that do.

**Can miss (approximate).**

- Quiet moves that leave a piece hanging **to a corridor PathClear** (victim on the path, dest elsewhere). Dest landings on the two-mover are hang-q (A+B). See remaining corridor ideas in [`IDEAS_TO_TRY.md`](../IDEAS_TO_TRY.md).
- **Cheap takes** below the loud floor that start a large exchange chain (except last-royal captures, which are still priced as ~8 material).
- Non-PV nodes only get **one** q ply → shallow recapture fights on scout windows.

### Quiescence: PathAware capture search (default)

Quiescence is capture-only **plus** promotions into two-movers / range capturers. Experimental modes (`Baseline`, `TopN`, `RecaptureOnly`, …) remain for harnesses; production default is **`QPruneMode::PathAware`**.

Harness note (post–Great General leaf, q=6): PathAware ~**37×** fewer q-nodes vs baseline (58 vs 2158) at nearly the same score; plain TopN ~**4.4×**; RecaptureOnly alone ~**200×** but over-pruned (score collapse) — not shipped alone.

#### Stand-pat and q transposition table

Stand-pat raises α; if stand-pat ≥ β the node returns immediately **unless** `prev_to` holds a major enemy (big / loud). Then dest recaptures of that square run first — they stay outside TopN and live delta — so a hanging GG is not scored as a keep on a null-window cutoff. A separate q TT (`1<<18`) stores bounds and the **best capture** for ordering. Unique-q HashSet tracking is **debug-only** (no release cost, does not prune).

#### Thin capture generation (victim square / cheap q-entry)

**Mechanism.**

- **Entry** into q with `prev_to`: generate captures **hitting** that square plus **loud SimpleTakes** (dest-captures of enemies ≥ loud floor) — **not** full-board `CapturesOnly` — plus any legal **loud promotions**.
- **Deep** PathAware plies: only captures hitting `prev_to` (directed landing emit for ordinary pieces; TwoStep / Free Eagle fall back to per-piece CapturesOnly + filter), plus loud promotions.
- PathClear / MultiLeg candidates that do not land on `prev_to` are filtered out at generation time as well (loud promotions exempt).
- Loud promotions skip PathAware top-N / delta / hang cuts so FreeKing→GG is not discarded as a “quiet” move.

**Rationale.** After a loud AB capture, the contested square is what matters; regenerating every capture on the board was the multi-second `rms` spike (`qST` blowups).

**Speedup.** Roughly **~1.5–3×** on loud-entry spikes for cheap entry gen; directed victim emit stacks another **~1.3–2×** on deep/victim gen. Parity harness in [`parity.rs`](parity.rs) gates victim-square equivalence vs full CapturesOnly+filter.

**Can miss (approximate relative to full CapturesOnly).** Loud PathClears / multi-leg snipes that do **not** land on the contested square (intentionally deferred to AB). Entry without `prev_to` still uses full CapturesOnly.

#### Top-N fanout and deep taper

| Constant | Value | Role |
|---|---:|---|
| `QUIESCE_TOP_N_PATH_AWARE_ROOT` | 2 | Max captures at the **first** q ply |
| `QUIESCE_TOP_N_DEEP` | 3 | Max captures at deeper q plies |
| `QUIESCE_PATHCLEAR_DEEP_BUDGET` | 1 | Max PathClear/MultiLeg among those |

Moves are ordered by last-royal instant wins, then total material captured (path-sum / tactical gain), then dest recapture, then net MVV-LVA; then truncated. Dest recaptures of a major `prev_to` stay outside TopN.

**Can miss.** A third (or later) equally good capture at the first q ply; corridor options beyond the budget.

#### PathClear / MultiLeg = destination recapture only

**Mechanism.** In PathAware q, PathClear and MultiLeg are expanded **only if** `mv.to == prev_to` (answering the previous landing). Comment in code: *“Q finishes the contested square; corridor wipes belong to AB.”*

**Can miss.** Capturing-range **mop-ups** along a ray that do not land on the last square; Free Eagle / two-step **snipes** off the contested square. Those lines must be found in main search or not at all within q.

#### Deep SimpleTakes = recapture onto `prev_to`

On deep PathAware plies, SimpleTakes that are not recaptures onto the previous landing are dropped.

**Can miss.** Starting a **new** loud take on another square mid-quiescence (q is meant to finish the current fight, not open a second front).

#### Hang skip inside quiescence

Same net/attack idea as AB hang: if net < `0.8 × mover` and the landing is already attacked, skip **before** make/unmake. PathClear/MultiLeg also get a **post-make** check in case the capture cleared defenders.

**Can miss.** Sound hanging sacrifices in q; rare pre-make false positives (partly mitigated by post-make for path/multi).

#### Material futility / live delta

After candidates are built, if `stand_pat + best_candidate_gain ≤ α`, return stand-pat. During the loop, individual moves with `stand_pat + gain ≤ α` are skipped. PathAware uses **net** gain (`enemy − own`); this does **not** model a further recapture of the mover (not a full SEE).

**Can miss.** Negative-SEE captures that are still correct for mate, promotion, or large positional reasons outside the material model.

### Move ordering (not a prune)

Captures use hang-aware MVV-LVA (`gain × 1000 − mover`, with hanging movers demoted). Killers (two quiet cutoffs per ply) and a history table (`from/to`, depth² bumps) order quiets. Root uses PVS with research on fail-high.

**Strength.** Ordering does not remove moves; bad order only costs nodes. The separate **AB hang prune** is what actually drops hanging high-value captures.

### Failure-mode cheat sheet

| Optimization | Typical missed / weakened line |
|---|---|
| Loud leaf gate | Quiet hanging piece taken by a corridor PathClear (dest elsewhere); pawn take that opens a big exchange |
| Non-PV q depth 1 | Multi-ply recapture fight on a scout window |
| PathClear dest-recapture only | GG corridor wipe not landing on the last square |
| Top-N = 2 at first q ply | Third good capture option at q entry |
| Deep SimpleTake recapture-only | New loud take on another square deeper in q |
| AB hang prune (mover ≥ 400) | Winning sac onto a “guarded” square |
| Stage-A β-cutoff | Quiet Lion / Free Eagle that was actually best |
| Null-move | Rare zugzwang; tactics needing q after a pass |
| LMR | Quiet setup that fails reduced search and never re-searches |
| Timed abort | Late root moves not reached at the deepest incomplete ID |

### Intentionally not implemented (yet)

- **Bitboards** — deferred; capability/oracle path preferred for exotic movers on 36×36.
- **King safety / check extensions** — mate is via royal capture / eval mate scores only; no in-check search extensions. See [`IDEAS_TO_TRY.md`](../IDEAS_TO_TRY.md) experiment L.
- **Soft root abort / root narrowing** on unlimited searches — full root every ID depth (GUI tree display is capped separately and does not affect the chosen move).
- **Full SEE**, aspiration windows, razoring, interior PVS, singular extensions.
