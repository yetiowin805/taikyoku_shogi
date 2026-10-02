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
| `scale-sample`, `*-grid` | Write checkpoint grids for Swiss (loud, PST, file-PST, two-mob, top4-mix, hang-q-ab, top11-c2, q-rs, …) |
| `mobility-seed` | Mobility-based init checkpoint |

Knockout is the tournament default (seeded 1v16 until stop). `--init-ratings` copies Glicko r/RD from a prior Swiss `ratings.json` or `state.json`.

## Eval / search history

Major eval or search-behavior merges need a `kind: logic` freeze (parent of the merge) via `./deploy/freeze_history.sh`. Weight-only bakes are `kind: weights`. Details: [`AGENTS.md`](../../AGENTS.md).

## Live and order-neutral ratings

Knockout Glicko ratings now update both players when their entire match finishes,
including all tiebreak games. Both updates use the same pre-update opponent
ratings. A serialized per-tree match ledger prevents replay on resume. Old closed
rounds are recognized as already rated; completed matches in still-open rounds
are caught up once on the next coordinator resume. Closing a round only applies
one passive RD update to entrants who did not participate, even if they are
playing in another tree. Play-ins retain the existing combined first-round
inactivity period. Glicko remains the seeding rating and is still order-dependent.

Every state save also writes `order-neutral-ratings.json` and includes its results
in `standings.md`. It fits an unregularized Bradley–Terry expected-score model to
all completed scored games in this run with equal game weights; draws contribute
half a win in each direction. It does not model a separate draw probability,
import prior ratings, weight recent results more, or change bracket seeding.
Ratings use the conventional 400-point logistic scale, centered at mean 1500.
Different engine revisions under one entrant name are deliberately pooled.

Before fitting, wins create winner-to-loser edges and draws create both edges.
The entire graph, including unplayed entrants, must be strongly connected for a
unique finite global fit (up to its fixed mean). Otherwise the report includes
strongly connected components and their directed edges and publishes no fitted
ratings. Disconnected schedules can be unidentifiable without infinite estimates;
one-way separation can send relative estimates to infinity. Neither is hidden
using priors or caps. See the [BradleyTerryScalable documentation](https://ellakaye.github.io/BradleyTerryScalable/articles/BradleyTerryScalable.html).

The deterministic Newton fit aggregates integer half-points first, verifies
convergence of predicted versus observed score totals, and reports failure rather
than outputting unconverged ratings. These are point estimates, not Glicko RD or
statistical evidence of strength differences. Repeated openings can correlate games.

For a read-only report from an existing run without restarting its coordinator:

```sh
cargo run --release --bin tournament_ratings -- path/to/state.json
```

Installing the live Glicko update behavior requires the tournament coordinator
to resume on the new build. Rolling game-engine updates alone do not replace the
running coordinator; do not edit its live state externally to simulate this.
