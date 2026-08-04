# Taikyoku Shogi Engine

A Rust game engine for **Taikyoku (Ultimate) Shogi**, a large historical Shogi variant on a **36×36 board**, with a local web GUI for play and debug.

## Features

- Full opening setup (~720 pieces) with ~303 piece types and movement configs
- Legal move generation, including two-step pieces, capturing-range generals, and Free Eagle multi-move patterns
- Promotion (zone + mandatory promotion for pawns, knights, etc.)
- Win by capturing all opponent royals (King / Crown Prince); draw by 500-move rule or insufficient material
- Self-play with heuristic (`mi`), random, royal-capture, or alpha-beta (`ab`) agents
- JSON game save / list / view under `games/`
- Versioned alpha-beta eval checkpoints under `models/`
- Interactive debug REPL (`debug`)
- **Local web UI** (`serve`): Play mode (human/AI per side) + Debug scrubber + log
- Stub UCI loop (handshake + first legal move only; not GUI-ready)

## Building

```bash
cargo build
cd web && npm install && npm run build && cd ..
```

## Running

### Local GUI (recommended)

```bash
# After building the web UI (see Building):
cargo run -- serve          # http://127.0.0.1:3000

# Frontend hot-reload during development:
#   terminal 1: cargo run -- serve
#   terminal 2: cd web && npm run dev   # http://127.0.0.1:5173 (proxies /api)
```

Open the UI, switch **Play** / **Debug**, load games from `games/`, click pieces to move.

In **Play**, the **Alpha-beta** panel sets depth / model / time for `ab`, and **Runs** can start `ab vs ab` (or vs `mi`) autoplay and stop/save when done.

### Self-play

```bash
cargo run -- play          # heuristic (MinimalIntelligencePlayer) — default
cargo run -- play mi       # same as above
cargo run -- play random   # uniform random legal moves
cargo run -- play royal    # prefer capturing royals
cargo run -- play ab       # alpha-beta (default depth 2, seed eval)
cargo run -- play ab --depth 2 --model models/ab-seed.json
cargo run -- export-seed   # write models/ab-seed.json
```

`ab` / `search` loads `models/ab-seed.json` when present (else built-in seed). Override with `--model`, `--depth`, or env `TAIKYOKU_AB_MODEL` / `TAIKYOKU_AB_DEPTH` / `TAIKYOKU_AB_TIME_MS`.

Games are saved as JSON under `games/`.

### List / view saved games

```bash
cargo run -- list
cargo run -- view games/game_1234567890.json
```

### Debug REPL

```bash
cargo run -- debug
```

Interactive REPL for iterating on engine/agent behavior against saved games:

- **Replay:** `load`, `forward`/`f`, `back`/`b`, `goto`/`g` (plies = MoveRecords; rebuild-from-start)
- **Inspect:** `board`, `pieces`, `piece`, `moves`, `check`, `attacked`, `status`/`info`
- **Edit:** `turn`, `place`, `remove`, `clear`, `reset` (edits snapshot a setup and branch)
- **Branch / agents:** `move …`, `suggest [mi|random|royal|ab]`, `play [mi|random|royal|ab]`, `save [file]`

Type `help` inside the REPL for full command syntax. Coordinates are shogi-style (file 1 = rightmost, rank 1 = top).

### UCI interface (stub)

```bash
cargo run
```

Responds to `uci`, `isready`, `ucinewgame`, `position startpos`, `position fen <TSFEN1…>`, `position … moves <TM1…>`, `go`, `quit`.  
`go` returns the first legal move as `bestmove` in **TM1** form. See **TSFEN / TM1** below.

### TSFEN / TM1 (position & move interchange)

Portable strings for sharing positions/moves between the engine, UCI, CLI, and tools. **JSON remains the on-disk canonical form** (`BoardPosition`, `GameRecordV2`); TSFEN/TM1 is the clipboard / protocol layer.

**Coordinates** are engine-native **0-based** `file,rank` (`0..=35`) — same as JSON. Shogi file/rank flip is display-only and is **not** used in TSFEN/TM1.

**Piece ids** are unique PascalCase `PieceType` names (`Pawn`, `GreatGeneral`, …), not board display symbols (those collide).

**TSFEN1** (one line):

```text
TSFEN1 <b|w> <draw> <n> <piece>…
# piece = B|W: [+]<PieceType>[<Base>] @ <file>,<rank>
# example piece: B:Pawn@5,2   W:+DragonKing<Rook>@18,30
```

Alias: `startpos` = opening setup.

**TM1** move tokens:

| Kind | Form |
|------|------|
| Standard | `12,3-15,6` or `12,3-15,6+` (promote) |
| Two-step | `12,3-13,4-15,6[+]` |
| Free Eagle | `12,3-13,4-14,5-15,6[+]` (from + path) |

CLI:

```bash
cargo run --release -- position tsfen --start opening
cargo run --release -- position tsfen --start data/raw/starts/SOME.json
cargo run --release -- position load-tsfen 'TSFEN1 b 0 …' --out /tmp/pos.json
cargo run --release -- game tsfen-moves data/raw/games/SOME.json
```

The GUI/`serve` game list includes `games/` and `data/raw/games/`; loading a v2 training game applies its embedded start position (not always the opening).

### Free Eagle sandbox

```bash
cargo run --bin test_free_eagle
```

Small-board REPL for experimenting with Free Eagle patterns.

## Testing

```bash
cargo test
```

## Coordinates

- Internal storage is **0-indexed** (files/ranks `0..=35`).
- Human-facing display often uses **1-indexed** values, and shogi-style viewers may flip file/rank for notation.
- **Black** advances toward **high** ranks; **White** toward **low** ranks.
- Promotion zone: Black ranks `25–35`, White ranks `0–10` (opponent’s last 11 ranks).

## Alpha-beta search (`ab`)

Implementation: [`src/search.rs`](src/search.rs). Defaults (from seed checkpoint / `SearchConfig`): **depth 2**, **quiescence depth 2**, **PathAware** capture pruning, no time limit. Override with `--depth`, `--model`, env `TAIKYOKU_AB_DEPTH` / `TAIKYOKU_AB_TIME_MS` / `TAIKYOKU_AB_QDEPTH`.

Ultimate Shogi has a huge branching factor (~hundreds of legal moves from the opening, many capturing-range “corridor” wipes). The search is therefore **selective**: it tries to look deep enough on contested lines while refusing to expand most quiet / mop-up capture trees. Several of the wins below are **approximate** — they can skip moves that would be considered in a full-width search. This section documents each significant optimization, why it exists, how much it helps when known, and what it can miss.

### Pipeline overview

1. **Iterative deepening** from depth 1 to the configured max; each iteration scores (almost) every legal root move.
2. Interior nodes use **alpha-beta** with a **transposition table**, **null-move pruning**, **late-move reductions**, and **staged move generation** (defer quiet multi-leg / Free Eagle until needed).
3. Captures that hang high-value pieces (e.g. Great General mopping a pawn onto a guarded square) are **skipped** in AB.
4. At depth 0, the engine does **not** always enter quiescence: only after a **loud** AB capture (enemy material ≥ loud floor ≈ **1200**). Quiets and cheap takes get **stand-pat eval**.
5. Quiescence is **capture-only**, default **PathAware**: thin generation onto the contested square, top-N fanout, PathClear only as dest-recapture, hang skips, and material futility.

Eval itself (capability material, rank PST, multi-royal bonuses) is separate; note that **Crown Prince / Drunken Elephant promotion-zone approach bonuses were removed** so the agent does not march royals into danger without king-safety awareness.

---

### Iterative deepening and time control

**Mechanism.** `search()` loops `d = 1..=depth`. Each completed iteration reorders the root move list (best first). If `max_time_ms` is set and the deadline fires mid-iteration, the search aborts and returns the **last fully completed** iteration (or a partial best only if no iteration finished).

**Rationale.** Deeper iterations inherit better move ordering; timed play needs a graceful stop without corrupting the previous result.

**Speedup.** Not a prune by itself; it enables timed search and makes selective search usable under a clock.

**Can miss.** Under a time limit, moves that would only appear as best at an **incomplete** deeper iteration are discarded. Unlimited searches (`max_time_ms: None`) do **not** soft-abort or narrow the root: every legal root move is searched each ID depth (modulo hang-pruned captures).

---

### Transposition table (main search)

**Mechanism.** Zobrist-keyed table (`1<<20` entries) stores Exact / Lower / Upper bounds with remaining depth and a preferred move. On probe, matching bounds cut the node; otherwise the TT move is tried first.

**Rationale.** Huge transposition rate on a 36×36 board with repetitive capture sequences; TT cutoffs and ordering are the largest “free” savings that stay correct when bounds are sound.

**Speedup.** Not isolated in harness numbers; essential for multi-iteration ID.

**Can miss.** Essentially nothing legality-wise. Hash collisions and depth-preferred replace can occasionally drop a useful shallower Exact entry (standard TT caveats). Bound cutoffs assume consistent eval.

---

### Null-move pruning

**Mechanism.** At interior nodes with `depth ≥ 2` (and β not in a mate score band), the side to move “passes”: turn flips, AB capture context is cleared, and a reduced search (`depth − 1 − R`, `R = 2`) runs. If that still fails high (≥ β), the node is cut off. When the reduced depth hits 0, the null child uses **stand-pat eval**, not quiescence. Consecutive nulls are disallowed.

**Rationale.** Ultimate Shogi almost always has near-null quiet moves, so true zugzwang is rare. Null-move is what makes opening depth-3 searches interactive in release builds.

**Speedup.** Part of the selective package that keeps opening `d3/q2` on the order of hundreds of milliseconds rather than multi-second full-width trees.

**Can miss (approximate).**
- Rare **zugzwang** / “only move loses” positions.
- Tactics that would only appear if the null-move leaf ran **quiescence** (null→depth 0 intentionally skips q).
- Mate-band positions skip null (correct caution).

---

### Late move reduction (LMR)

**Mechanism.** Late quiet non-killers (interior) and late non-captures (root) are searched at reduced depth once move index is high enough (`≥ 3`, depth ≥ 2; reduction 1, or 2 for very late moves). If the reduced score fails high (> α), the move is **re-searched at full depth**. Captures and killer moves are never reduced. Root also uses PVS (null-window then research).

**Rationale.** After TT / captures / killers, most remaining quiets do not change the PV; cutting their depth is the main way to survive large root fanout at d3/d4.

**Speedup.** Bundled with null-move / non-PV q caps for deep ID interactivity; no isolated × factor in comments.

**Can miss (approximate).** Quiet **setup / tempo** moves that look mediocre at reduced depth and never trigger re-search (false fail-low). Captures are safe from this path.

---

### Staged move generation (quiet multi-leg deferred)

**Mechanism.** Interior AB first generates `WithoutQuietMultiLeg`: captures, ordinary quiets, and **capturing** multi-leg / Free Eagle. Only if that stage does **not** β-cutoff does it generate `QuietMultiLegOnly` (quiet two-step / quiet Free Eagle). If stage A is empty, stage B runs alone.

**Rationale.** Quiet Lion / Free Eagle trees are expensive to generate and rarely needed once a capture or simple quiet already cuts.

**Speedup.** Defers the heaviest quiet generators; capture-only gen is separately measured faster than full gen on the opening.

**Can miss (approximate).** When stage A **does** β-cutoff, quiet multi-leg / Free Eagle maneuvers are **never searched** at that node. A quiet Lion reposition that was actually best can be invisible if some earlier stage-A move already looked good enough to cut.

---

### AB hang prune (high-value hanging captures)

**Constants.** `HIGH_VALUE_HANGER = 400`, `HANG_NET_FRAC = 0.8`.

**Mechanism.** Skip a capture in AB (root and interior) when:
1. the mover’s material value is ≥ 400 (capturing-range generals and similar),
2. net path material `(enemy − own) < 0.8 × mover`, and
3. the landing square is attacked by the opponent.

Root PathClear / MultiLeg use a **post-fire** (simulate) attack check; interior uses a cheaper **pre-move** landing attack check.

**Rationale.** After capturing-range tariffs were raised, Great General–class pieces love to “mop” low pieces onto guarded squares. Expanding those lines dominates the tree and is almost always losing.

**Speedup.** Large practical win on midgame positions full of GG/BG corridors (not a single published ×; removes entire classes of root/AB children).

**Can miss (approximate).**
- **Sound sacrifices** where net < 0.8× mover but the position is winning (deflection, discovery, forcing lines).
- Interior **false positives**: a PathClear that **removes** the defender of the landing can look “hanging” pre-move and be skipped even though post-fire it would be safe.
- Pieces valued **below 400** are never hang-pruned this way, even if they hang badly.

---

### Leaf quiescence gating (loud captures only)

**Loud floor.** `seed_loud_capture_floor()` ≈ `max(500×2.4, 50×8) = 1200` (from capturing-range / jump tariffs in [`src/eval.rs`](src/eval.rs)).

**Mechanism.** At AB depth 0:
- If the parent AB move’s captured enemy material `< 1200` (quiet move or cheap take) → **stand-pat eval**, no quiescence.
- Otherwise enter quiescence with `prev_to` = the AB landing square.
- **Non-PV** (null-window) leaves use `qdepth = min(config, 1)`; PV / fail-high research uses the full configured q depth.
- Null-move children clear the loud-capture flag, so they do not gate into q via that path.

**Rationale.** Full capture quiescence after every quiet leaf exploded wall time (especially SimpleTake trees). Q should only **resolve the exchange** after something loud already happened in AB.

**Speedup.** Primary reason deep ID stays interactive: most leaves never touch q. PathAware then thins the leaves that do.

**Can miss (approximate).**
- Quiet moves that leave a piece **hanging** (horizon effect; no capture search).
- **Cheap takes** below 1200 that start a large exchange chain.
- Non-PV nodes only get **one** q ply → shallow recapture fights on scout windows.

---

### Quiescence: PathAware capture search (default)

Quiescence is capture-only. Experimental modes (`Baseline`, `TopN`, `RecaptureOnly`, …) remain for harnesses; production default is **`QPruneMode::PathAware`**.

Harness note (post–Great General leaf, q=6): PathAware ~**37×** fewer q-nodes vs baseline (58 vs 2158) at nearly the same score; plain TopN ~**4.4×**; RecaptureOnly alone ~**200×** but over-pruned (score collapse) — not shipped alone.

#### Stand-pat and q transposition table

Stand-pat raises α; if stand-pat ≥ β the node returns immediately. A separate q TT (`1<<18`) stores bounds and (now) the **best capture** for ordering. Unique-q HashSet tracking is **debug-only** (no release cost, does not prune).

#### Thin capture generation (victim square / cheap q-entry)

**Mechanism.**
- **Entry** into q with `prev_to`: generate captures **hitting** that square plus **loud SimpleTakes** (dest-captures of enemies ≥ loud floor) — **not** full-board `CapturesOnly`.
- **Deep** PathAware plies: only captures hitting `prev_to` (directed landing emit for ordinary pieces; TwoStep / Free Eagle fall back to per-piece CapturesOnly + filter).
- PathClear / MultiLeg candidates that do not land on `prev_to` are filtered out at generation time as well.

**Rationale.** After a loud AB capture, the contested square is what matters; regenerating every capture on the board was the multi-second `rms` spike (`qST` blowups).

**Speedup.** Roughly **~1.5–3×** on loud-entry spikes for cheap entry gen; directed victim emit stacks another **~1.3–2×** on deep/victim gen (from recent speedup work). Parity harness in [`src/parity.rs`](src/parity.rs) gates victim-square equivalence vs full CapturesOnly+filter.

**Can miss (approximate relative to full CapturesOnly).** Loud PathClears / multi-leg snipes that do **not** land on the contested square (intentionally deferred to AB). Entry without `prev_to` still uses full CapturesOnly.

#### Top-N fanout and deep taper

| Constant | Value | Role |
|----------|------:|------|
| `QUIESCE_TOP_N_PATH_AWARE_ROOT` | 2 | Max captures at the **first** q ply |
| `QUIESCE_TOP_N_DEEP` | 3 | Max captures at deeper q plies |
| `QUIESCE_PATHCLEAR_DEEP_BUDGET` | 1 | Max PathClear/MultiLeg among those |

Moves are ordered by landing victim, then recapture flag, then net MVV-LVA; then truncated.

**Can miss.** A third (or later) equally good capture at the first q ply; corridor options beyond the budget.

#### PathClear / MultiLeg = destination recapture only

**Mechanism.** In PathAware q, PathClear and MultiLeg are expanded **only if** `mv.to == prev_to` (answering the previous landing). Comment in code: *“Q finishes the contested square; corridor wipes belong to AB.”*

**Can miss.** Capturing-range **mop-ups** along a ray that do not land on the last square; Free Eagle / two-step **snipes** off the contested square. Those lines must be found in main search or not at all within q.

#### Deep SimpleTakes = recapture onto `prev_to`

On deep PathAware plies, SimpleTakes that are not recaptures onto the previous landing are dropped.

**Can miss.** Starting a **new** loud take on another square mid-quiescence (q is meant to finish the current fight, not open a second front).

#### Hang skip inside quiescence

Same net/attack idea as AB hang: if net < `0.8 × mover` and the landing is already attacked, skip **before** make/unmake (SimpleTake / PathClear / MultiLeg). PathClear/MultiLeg also get a **post-make** check in case the capture cleared defenders.

**Can miss.** Sound hanging sacrifices in q; rare pre-make false positives (partly mitigated by post-make for path/multi).

#### Material futility / live delta

After candidates are built, if `stand_pat + best_candidate_gain ≤ α`, return stand-pat. During the loop, individual moves with `stand_pat + gain ≤ α` are skipped. PathAware uses **net** gain (`enemy − own`); this does **not** model a further recapture of the mover (not a full SEE).

**Can miss.** Negative-SEE captures that are still correct for mate, promotion, or large positional reasons outside the material model.

---

### Move ordering (not a prune)

Captures use hang-aware MVV-LVA (`gain × 1000 − mover`, with hanging movers demoted). Killers (two quiet cutoffs per ply) and a history table (`from/to`, depth² bumps) order quiets. Root uses PVS with research on fail-high.

**Strength.** Ordering does not remove moves; bad order only costs nodes. The separate **AB hang prune** is what actually drops hanging high-value captures.

---

### Failure-mode cheat sheet

| Optimization | Typical missed / weakened line |
|--------------|--------------------------------|
| Loud leaf gate | Quiet hanging piece; pawn take that opens a big exchange |
| Non-PV q depth 1 | Multi-ply recapture fight on a scout window |
| PathClear dest-recapture only | GG corridor wipe not landing on the last square |
| Top-N = 2 at first q ply | Third good capture option at q entry |
| Deep SimpleTake recapture-only | New loud take on another square deeper in q |
| AB hang prune (mover ≥ 400) | Winning sac onto a “guarded” square; PathClear that clears the guard |
| Stage-A β-cutoff | Quiet Lion / Free Eagle that was actually best |
| Null-move | Rare zugzwang; tactics needing q after a pass |
| LMR | Quiet setup that fails reduced search and never re-searches |
| Timed abort | Late root moves not reached at the deepest incomplete ID |

---

### Intentionally not implemented (yet)

- **Bitboards** — deferred; capability/oracle path preferred for exotic movers on 36×36.
- **King safety / check extensions** — mate is via royal capture / eval mate scores only; no in-check search extensions. Royal **position** (CP/DE approach) bonuses were removed for this reason.
- **Soft root abort / root narrowing** on unlimited searches — full root every ID depth (GUI tree display is capped separately and does not affect the chosen move).
- **Full SEE**, aspiration windows, razoring, interior PVS, singular extensions.

## Current scope

| Area | Status |
|------|--------|
| Piece set + opening | Largely complete |
| Move generation / apply | Working |
| Heuristic / random / royal / ab self-play | Working |
| Alpha-beta + PathAware quiescence (`models/`) | Working (default depth 2, q 2; selective — see above) |
| Debug + JSON history | Working (replay / edit / branch / agents) |
| Local web GUI | Working (Play + Debug + log) |
| UCI | Stub (TSFEN1 / TM1 position+moves) |
| TSFEN / TM1 interchange | Working (`src/notation.rs`) |
| Continuous cloud self-play | Working (`worker daemon` + [`deploy/`](deploy/README.md)) |

## Continuous Texel workers (cloud)

For overnight self-play on a ~$20/mo VPS (Hetzner first; Oracle Always Free optional later), see **[`deploy/README.md`](deploy/README.md)**.

Summary:

```bash
# Generate starts, then run the continuous worker (writes data/run/status.json)
cargo run --release -- pool generate --count 128
cargo run --release -- worker daemon --batch 8 --jobs 4 --black ab --white ab --starts data/raw/starts

# Inspect: cat data/run/status.json
# Or: serve + SSH tunnel → GET /api/training/status
# Stop: SIGTERM / systemctl stop / touch data/run/STOP
```

systemd unit and env example live under [`deploy/systemd/`](deploy/systemd/) and [`deploy/worker.env.example`](deploy/worker.env.example).
