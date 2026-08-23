# Training pipeline

Local Texel-style loop: generate starts, play games, featurize, fit, then compare agents in Swiss / knockout. Cloud deploy (systemd, VPS) is in [`deploy/README.md`](../../deploy/README.md). History freeze rules are in [`AGENTS.md`](../../AGENTS.md).

```bash
cargo run --            # prints training subcommands at the bottom of usage
```

Agents: `mi`, `random`, `royal`, `ab`. Starts: `opening` | `random` | `light` | a directory of pool JSON.

## Data layout

| Path | Role |
|---|---|
| `data/raw/games` | Played games (`GameRecordV2` JSON) |
| `data/raw/starts` | Start-position pool |
| `data/derived/positions` | Featurized rows (disposable) |
| `data/run/status.json` | Daemon progress |
| `data/run/STOP` | Cooperative daemon stop |

Constants live in [`paths.rs`](paths.rs).

## Typical loop

```bash
cargo run --release -- pool generate --count 128
cargo run --release -- worker daemon --batch 8 --jobs 4 --black ab --white ab --starts data/raw/starts
# SIGTERM / systemctl stop / touch data/run/STOP

cargo run --release -- featurize
cargo run --release -- texel-fit --features data/derived/positions --out models/texel.json
```

Inspect: `cat data/run/status.json`, or `serve` + SSH tunnel → `GET /api/training/status`.

`--time-ms` is a soft AB budget (last completed ID depth). Omit `--depth` → ceiling 8. `--seed-base 0` = per-game OS entropy.

## Subcommands (short)

| Command | Purpose |
|---|---|
| `worker run` / `batch` / `daemon` | One game, N games, or continuous batches |
| `pool generate` | Fischer shuffle + ablations (or `--from-play` legacy midgames) |
| `featurize` | Event-driven sampling into `data/derived/positions` |
| `eval-trace` | Rank interesting eval swings in saved games |
| `texel-fit` | Logistic Texel on featurized rows (default: range two-movers + capturers only) |
| `match` / `tournament` | Head-to-head; RR / Swiss / knockout (`--format`) |
| `scale-sample`, `*-grid` | Write checkpoint grids for Swiss (loud, PST, file-PST, two-mob, top4-mix, hang-q-ab, top11-c2, …) |
| `mobility-seed` | Mobility-based init checkpoint |

Knockout is the tournament default (seeded 1v16 until stop). `--init-ratings` copies Glicko r/RD from a prior Swiss `ratings.json` or `state.json`.

## Eval / search history

Major eval or search-behavior merges need a `kind: logic` freeze (parent of the merge) via `./deploy/freeze_history.sh`. Weight-only bakes are `kind: weights`. Details: [`AGENTS.md`](../../AGENTS.md).
