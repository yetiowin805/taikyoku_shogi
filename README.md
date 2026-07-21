# Taikyoku Shogi Engine

A Rust game engine for **Taikyoku (Ultimate) Shogi**, a large historical Shogi variant on a **36×36 board**. Interaction is CLI-only (no graphical UI yet).

## Features

- Full opening setup (~720 pieces) with ~303 piece types and movement configs
- Legal move generation, including two-step pieces, capturing-range generals, and Free Eagle multi-move patterns
- Promotion (zone + mandatory promotion for pawns, knights, etc.)
- Win by capturing all opponent royals (King / Crown Prince); draw by 500-move rule or insufficient material
- Self-play with a heuristic player or uniform random moves
- JSON game save / list / view under `games/`
- Interactive debug REPL (`debug`)
- Stub UCI loop (handshake + first legal move only; not GUI-ready)

## Building

```bash
cargo build
```

## Running

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

### Debug tool

```bash
cargo run -- debug
```

Interactive REPL for loading games, stepping moves, listing legal moves, and attack/check queries.

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
| Debug + JSON history | Working |
| UCI / search engine | Stub / absent |
| Graphical UI | None |
