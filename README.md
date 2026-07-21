# Taikyoku Shogi Engine

A Rust game engine for **Taikyoku (Ultimate) Shogi**, a large historical Shogi variant on a **36×36 board**, with a local web GUI for play and debug.

## Features

- Full opening setup (~720 pieces) with ~303 piece types and movement configs
- Legal move generation, including two-step pieces, capturing-range generals, and Free Eagle multi-move patterns
- Promotion (zone + mandatory promotion for pawns, knights, etc.)
- Win by capturing all opponent royals (King / Crown Prince); draw by 500-move rule or insufficient material
- Self-play with a heuristic player or uniform random moves
- JSON game save / list / view under `games/`
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

### Self-play

```bash
cargo run -- play          # heuristic (MinimalIntelligencePlayer) — default
cargo run -- play mi       # same as above
cargo run -- play random   # uniform random legal moves
```

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
- **Branch / agents:** `move …`, `suggest [mi|random|royal]`, `play [mi|random|royal]`, `save [file]`

Type `help` inside the REPL for full command syntax. Coordinates are shogi-style (file 1 = rightmost, rank 1 = top).

### UCI interface (stub)

```bash
cargo run
```

Responds to basic UCI commands (`uci`, `isready`, `ucinewgame`, `position startpos`, `go`, `quit`).  
`go` returns the first legal move as `bestmove`. FEN and move-string parsing are not implemented.

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

## Current scope

| Area | Status |
|------|--------|
| Piece set + opening | Largely complete |
| Move generation / apply | Working |
| Heuristic / random self-play | Working |
| Debug + JSON history | Working (replay / edit / branch / agents) |
| Local web GUI | Working (Play + Debug + log) |
| UCI / search engine | Stub / absent |
