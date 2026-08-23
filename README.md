# Taikyoku Shogi Engine

A Rust engine for **Taikyoku (Ultimate) Shogi** — 36×36 board, ~720 pieces, ~303 types — with a local web GUI for play and debug.

## Features

- Opening setup, legal move generation (two-step pieces, capturing-range generals, Free Eagle), promotion
- Win by capturing all opponent royals (King / Crown Prince); draw by 100-move rule or insufficient material
- Agents: heuristic (`mi`), random, royal-capture, alpha-beta (`ab`)
- Local web UI, debug REPL, JSON game history
- Versioned eval checkpoints under `models/`; Texel / Swiss / knockout training pipeline

## Building

```bash
cargo build
cd web && npm install && npm run build && cd ..
```

## Running

**GUI (recommended):**

```bash
cargo run -- serve          # http://127.0.0.1:3000
```

See [`web/README.md`](web/README.md) for hot-reload and Play / Debug usage.

**Self-play:**

```bash
cargo run -- play          # heuristic (mi)
cargo run -- play ab --depth 2 --model models/ab-seed.json
cargo run -- export-seed   # write models/ab-seed.json
```

`ab` loads `models/ab-seed.json` when present. Override with `--model`, `--depth`, or `TAIKYOKU_AB_MODEL` / `TAIKYOKU_AB_DEPTH` / `TAIKYOKU_AB_TIME_MS`. Games save under `games/`.

```bash
cargo run -- list
cargo run -- view games/game_1234567890.json
cargo run -- debug         # REPL; type help
cargo run --               # UCI stub (TSFEN1 / TM1)
cargo test
```

CLI usage: `cargo run --` with no args that match a subcommand, or any bad flag, prints the full command list (including training). Free Eagle sandbox: `cargo run --bin test_free_eagle`.

## Docs

| Where | What |
|---|---|
| [`src/README.md`](src/README.md) | Coordinates, TSFEN/TM1, debug REPL, alpha-beta / PathAware q (what it can miss) |
| [`src/training/README.md`](src/training/README.md) | Worker, Texel, grids, Swiss / knockout |
| [`web/README.md`](web/README.md) | Local GUI |
| [`deploy/README.md`](deploy/README.md) | Cloud workers, systemd, VPS |
| [`AGENTS.md`](AGENTS.md) | Git branches, eval/search history freeze |
| [`IDEAS_TO_TRY.md`](IDEAS_TO_TRY.md) | Search / eval experiments from knockout games (not implemented) |

## Current scope

| Area | Status |
|---|---|
| Piece set, move gen, apply | Working |
| `mi` / random / royal / `ab` self-play | Working |
| Alpha-beta + PathAware q (`models/`) | Working (selective — see [`src/README.md`](src/README.md)) |
| Debug + JSON history | Working |
| Local web GUI | Working |
| UCI | Stub |
| TSFEN / TM1 | Working ([`src/notation.rs`](src/notation.rs)) |
| Continuous self-play | Working ([`deploy/README.md`](deploy/README.md)) |
