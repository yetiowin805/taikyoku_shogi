# Cloud continuous Texel workers

Run self-play on a cheap Linux VPS (Hetzner first), accumulate games under `data/raw/games`, then featurize / texel-fit on a quiet window. Same binary and systemd unit work later on a second host (including Oracle Always Free).

Local worker / Texel / tournament CLI: [`src/training/README.md`](../src/training/README.md).

## Host recommendation

**Primary: Hetzner Cloud** (~€6–12/mo, under $20):

- Prefer **CAX21** (Arm, 4 vCPU / 8 GB) or **CX32** (x86, 4 vCPU / 8 GB)
- Ubuntu LTS, public IPv4, 40–80 GB disk
- Firewall: SSH (22); do **not** expose the GUI port publicly — use an SSH tunnel or Tailscale

**Optional later: Oracle Always Free** (Ampere ~2 OCPU / 12 GB after mid-2026 cuts). Signup/capacity is flaky. If you get an instance, install the **same** unit with a different `SEED_BASE` (or separate `OUTDIR`) and sync games back to the primary host. Do not design around OCI-specific APIs; treat Free as bonus CPU.

## Outside checklist (human)

1. Create the VPS + SSH key; open only port 22 (and Tailscale if used).
2. Create a system user: `sudo useradd -r -m -d /opt/taikyoku_shogi taikyoku` (or clone as that user under `/opt/taikyoku_shogi`).
3. Install a Rust toolchain (`rustup`) **or** copy a release binary built on matching arch.
4. Clone this repo into `/opt/taikyoku_shogi`, `cargo build --release`.
5. Ensure `models/ab-seed.json` (or your current model) is present; set `MODEL=` in the env file if needed.
6. Set `STARTS=random` (Fischer mirrored rank shuffle + powerful/royal ablations, fresh each game). Or `pool generate --count 128` and `STARTS=data/raw/starts`.
7. Set `SEED_BASE=0` for per-game OS-random seeds (default in the example env). Use a positive `SEED_BASE` only when you need a reproducible `N+index` sequence.
8. Install systemd unit + env (below); `systemctl enable --now taikyoku-worker`.
9. Calibrate: one short batch with `--jobs` equal to vCPUs, note wall time → games/hour.

## Install systemd

```bash
sudo mkdir -p /etc/taikyoku
sudo cp deploy/worker.env.example /etc/taikyoku/worker.env
sudo $EDITOR /etc/taikyoku/worker.env

sudo cp deploy/systemd/taikyoku-worker.service /etc/systemd/system/
# Optional: browse games via SSH tunnel without rsync
sudo cp deploy/systemd/taikyoku-serve.service /etc/systemd/system/
# Optional weekly starts refresh:
sudo cp deploy/systemd/taikyoku-pool.service deploy/systemd/taikyoku-pool.timer /etc/systemd/system/
# Daily game-count digest (skipped if training/match already notified ~same day):
sudo cp deploy/systemd/taikyoku-daily-digest.service deploy/systemd/taikyoku-daily-digest.timer /etc/systemd/system/
# Optional: every-N-games milestone instead/in addition:
# sudo cp deploy/systemd/taikyoku-notify-milestone.service deploy/systemd/taikyoku-notify-milestone.timer /etc/systemd/system/
sudo cp deploy/notify.env.example /etc/taikyoku/notify.env
# configure SMTP (msmtp) or NTFY_TOPIC in /etc/taikyoku/notify.env

sudo systemctl daemon-reload
sudo systemctl enable --now taikyoku-worker
# sudo systemctl enable --now taikyoku-serve
# sudo systemctl enable --now taikyoku-pool.timer
# sudo systemctl enable --now taikyoku-daily-digest.timer
```

Make deploy scripts executable after clone: `chmod +x deploy/*.sh`.

Control:

```bash
sudo systemctl stop taikyoku-worker    # SIGTERM → drain current batch, then exit
sudo systemctl start taikyoku-worker
sudo systemctl status taikyoku-worker
journalctl -u taikyoku-worker -f
```

Alternate stop without systemd: `touch data/run/STOP` (cleared when the daemon exits).

## Status and inspection

Progress file (updated every batch):

```bash
cat data/run/status.json
# games_completed, games_failed, last_game_id, disk_free_gb, config, running
```

Count finished games: `ls data/raw/games/*.json 2>/dev/null | wc -l`

Aborted mid-game runs (e.g. illegal/`make_move` failures) are written under **`data/raw/games/partial/`** as the same `GameRecordV2` JSON with `abort_reason` set and moves played so far — listed in the GUI alongside finished games.

### GUI via SSH tunnel (recommended)

Enable the serve unit (binds **127.0.0.1:3000** only):

```bash
sudo systemctl enable --now taikyoku-serve
```

Or run once: `./target/release/taikyoku_shogi serve 3000`

On your laptop:

```bash
ssh -L 3000:127.0.0.1:3000 user@vps
# open http://127.0.0.1:3000 — load games from data/raw/games via the GUI
```

No full `rsync` required to browse. Pull only what you need with e.g. `rsync -avz --include='*.json' --latest …` if you want a local copy.

## Overnight multi-regimen tournament

Fit several Texel hyperparameter settings on the same features, then round-robin them (plus seed) with Elo. Default `--games-per-pair 24` is intentionally large so it keeps going until you cancel; the schedule interleaves matchups so pairs stay roughly even. Stop anytime; resume later.

```bash
# one-shot (stops worker, fits, then tours)
chmod +x deploy/*.sh
cargo build --release
nohup ./deploy/run_overnight_tourney.sh --jobs "$(nproc)" \
  >data/run/tourney.log 2>&1 &

# stop (aborts in-flight games, writes standings so far + ntfy)
touch data/run/TOURNEY_STOP

# resume
nohup ./deploy/run_overnight_tourney.sh --resume --run-id tourney-YYYYMMDDTHHMMSSZ --skip-fit \
  >data/run/tourney.log 2>&1 & disown
```

Starts: `light` = shuffle-only ranks without powerful/royals (no ablations). Standings live under `data/raw/tourney/<run_id>/standings.md`.

```bash
# latest run: standings + knockout trees
./deploy/tourney_status.py
# latest top11-c2 run; trees only; one tree
./deploy/tourney_status.py top11-c2
./deploy/tourney_status.py --tree
./deploy/tourney_status.py --tree 1
```

Incomplete exits (stop file / aborted slots) now fail the tournament binary with a non-zero status so notify says stopped/incomplete rather than “done”.

### Loud-grid continuous Glicko Swiss (preferred)

3×3×3 material grid (Hook H ∈ {90,100,110}% × Capricorn C ∈ {80,100,120}% × other two-movers O ∈ {80,100,110}%; capturers stay at seed; center `H100C100O100` = seed). Continuous Swiss uses an elite pool (`r + RD + RD_leader >` current max `r`, always including the top 2 by `r`), prioritizes agents below 4 counted games before elite gating, inflates sit-out RD every 10 finished games, keeps at least 2 rating-window opponents (expand by alternating higher/lower from the closer side), and after 4 games prefers not to rematch someone who already has more than half of an agent's games. Once every agent has 4 games, 10% of pairings are the current leader vs a random opponent who fails the UCI elite bar. Prefer `--detach` (or the systemd unit) so SSH logout cannot kill the job.

**One-shot as root** (pulls `main`, builds with the `taikyoku` user’s cargo PATH, regenerates grid, starts 1s/move Swiss):

```bash
cd /opt/taikyoku_shogi
./deploy/pull_build_loud_swiss.sh
# defaults: --time-ms 1000 --depth 8 --detach --jobs $(nproc)
tail -f data/run/loud-swiss.log
```

Or after a local `cargo build --release`:

```bash
# fresh (detach from terminal); soft 1s budget + depth ceiling 8
./deploy/run_loud_swiss.sh --detach --jobs "$(nproc)" --time-ms 1000 --depth 8

# resume
./deploy/run_loud_swiss.sh --detach --resume --run-id loud-swiss-YYYYMMDDTHHMMSSZ --skip-gen

pgrep -af tournament
tail -f data/run/loud-swiss.log
```

Stop with `touch data/run/TOURNEY_STOP`. Cooperative stop exits 0 (continuous Swiss has no “done” round count). Overlapping tournament processes are refused (`exit 3`).

Optional systemd (survives reboot if enabled):

```bash
sudo cp deploy/tourney.env.example /etc/taikyoku/tourney.env
sudo cp deploy/systemd/taikyoku-tourney.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now taikyoku-tourney
# stop: sudo touch /opt/taikyoku_shogi/data/run/TOURNEY_STOP
#   or: sudo systemctl stop taikyoku-tourney
```

### PST-grid continuous Glicko Swiss

3×3×3 fast rank-PST grid (promo P ∈ {110,120,130}% × opp-half H ∈ {25,50,75}% of mid→promo gap × back B ∈ {25,50,75}%; slow PST / material unchanged):

```bash
cd /opt/taikyoku_shogi
./deploy/pull_build_pst_swiss.sh
# defaults: --time-ms 1000 --depth 8 --detach --jobs $(nproc)
tail -f data/run/pst-swiss.log
```

Or after a local release build:

```bash
./deploy/run_pst_swiss.sh --detach --jobs "$(nproc)" --time-ms 1000 --depth 8
```

### File-PST continuous Glicko Swiss

5×3×3 grid (45 agents): file F{edge}C{center} ∈ {100/100, 90/110, 80/120, 70/150, 50/200} × back B ∈ {50,60,75}% × tropism T ∈ {10,15,20} (`eg_tropism_scale` 1.0/1.5/2.0). Seed cell `F100C100B60T15` = seed copy. File PST applies to pieces with both wing dirs except royals and an enumerated asymmetric denylist.

```bash
cd /opt/taikyoku_shogi
./deploy/pull_build_file_pst_swiss.sh
tail -f data/run/file-pst-swiss.log
```

### Two-mover mobility Swiss

18 C×K×A cells (curve Lin/Sqrt/Logi × k 40/100/200 × apply raw/rank; file apply omitted — seed `file_factor` is flat) plus `SEED` (mobility off, B65/T12) and history `BASE_*` / `LOGIC_*` from [`models/history/manifest.json`](../models/history/manifest.json). Freeze pinned binaries first (`./deploy/freeze_history.sh`).

```bash
cd /opt/taikyoku_shogi
./deploy/pull_build_two_mob_swiss.sh
tail -f data/run/two-mob-swiss.log
```

### Top-11 Texel inspection fits

Eleven chassis from the mix-tournament top 11, each `texel-fit --only-large` with `--init` = that chassis. Same mix-game features. Only range two-movers + range capturers move; mid table / PST / tropism / `k` stay on the parent. Then `deploy/compare_top11_texel.py` writes a /Pawn %Δ table. Re-fit without re-sampling: `--skip-featurize`.

```bash
cd /opt/taikyoku_shogi
# latest mix run if you omit --games-dir
./deploy/run_top11_texel_fits.sh \
  --games-dir data/raw/tourney/top4-mix-swiss-20260820T061654Z
# table: models/top11-texel/compare.md
# re-print: ./deploy/run_top11_texel_fits.sh --compare-only
```

Do **not** bake onto `models/ab-seed.json`. This is the look-at-deltas step before a large-piece transplant / 22-agent knockout (see [`FUTURE_NOTES.md`](../FUTURE_NOTES.md)).

### Top-11 + C2 history knockout

Mix-tournament top 11, leftover playable history (`BASE_PRELOUD`, `BASE_T150C50`, `BASE_T150C120`, `BASE_H105O105`, `BASE_P120H75B60`, `LOGIC_H105`, `LOGIC_HANGQ_ANY`), and C2K50A1 mobility twins of every weight chassis except `C2K50A1` and `SEED` (`C2K100A1D50` keeps D50 → `C2K50A1D50`). 32 agents. Same 1s / depth-8 PathAware setup as the mix tourney. Freeze pinned binaries first (`./deploy/freeze_history.sh`).

```bash
cd /opt/taikyoku_shogi
./deploy/pull_build_top11_c2_swiss.sh --branch cursor/top11-c2-swiss
tail -f data/run/top11-c2-swiss.log
```

### Hang-q A/B mini knockout

Four mix-tournament weights (`T150_P120_T12`, `H120_P120_T15`, `AVG_T150_H120`, `C2K50A1`) × current / dest-MultiLeg A / dest-PathClear B / AB (16 agents). Same 1s / depth-8 PathAware setup as the mix tourney.

```bash
cd /opt/taikyoku_shogi
./deploy/pull_build_hang_q_ab_swiss.sh --branch cursor/hang-q-ab-flags
tail -f data/run/hang-q-ab-swiss.log
```

### R / S1 / S2 search-twin knockout

Seven experimental chassis (`H120_P120_T15`, `AVG_P120_SEED_C2`, `SEED`, `BASE_P120H50B75_C2`, `T150_P120_T12_C2`, `C2K100A1D50`, `C2K50A1`) each with current / R / S1 / S2 search twins, plus five leftover-history baselines (`BASE_T150C120`, `BASE_P120H75B60`, `LOGIC_HANGQ_ANY`, `BASE_T150C50`, `BASE_H105O105`). 33 agents. Same 1s / depth-8 PathAware setup. Freeze pinned binaries first (`./deploy/freeze_history.sh`). Do not bake R/S onto `models/ab-seed.json`.

```bash
cd /opt/taikyoku_shogi
./deploy/pull_build_q_rs_swiss.sh --branch cursor/q-rs-grid
tail -f data/run/q-rs-swiss.log
```

### Scale-sample Swiss (legacy ±10% samples)

```bash
./deploy/run_scale_swiss.sh --detach --jobs "$(nproc)"
# resume: --detach --resume --run-id swiss-… --skip-gen
```

Note: `tournament --format swiss` is now continuous Glicko (finite `--swiss-rounds` is ignored). Correlate multipliers vs score / rating:

```bash
python3 deploy/scale_sample_corr.py \
  --samples models/scale-sample/samples.json \
  --state data/raw/tourney/swiss-YYYYMMDDTHHMMSSZ/state.json
# optional: --metric elo   --include-controls
```

### Notifications (email / ntfy)

Install `msmtp` (or use [ntfy.sh](https://ntfy.sh)) and set `/etc/taikyoku/notify.env` from `deploy/notify.env.example` (`NOTIFY_TO=…` is prefilled in the example).

Three kinds of mail/push:

1. **Training** — `deploy/run_texel_cycle.sh` emails games count, ~mean moves, position count, avg pos/game, model path, loss line.
2. **Strength test** — `deploy/run_match.sh` emails model A vs B, pairs/depth, and scoreboard from the match log.
3. **Daily digest** — `taikyoku-daily-digest.timer` emails finished/partial game counts + worker status. Skipped if training or match already stamped `data/run/last_notify_activity` within ~20h (`FORCE=1` to send anyway).

Kick off a fit and walk away:

```bash
# worker can keep running during featurize/fit
nohup ./deploy/run_texel_cycle.sh \
  --games-dir data/raw/games \
  --out-model models/ab-texel-v1.json \
  >data/run/texel_cycle.log 2>&1 &
```

Match (stop the worker first — CPU-heavy, long games):

```bash
sudo systemctl stop taikyoku-worker
nohup ./deploy/run_match.sh \
  --model-a models/ab-seed.json --model-b models/ab-texel-v1.json \
  --games 16 --depth 2 \
  >data/run/match.log 2>&1 &
```

Manual daily: `FORCE=1 ./deploy/daily_digest.sh`

### HTTP status without the full GUI

With `serve` running (tunnel as above):

```bash
curl -s http://127.0.0.1:3000/api/training/status | jq .
```

Returns the contents of `data/run/status.json` (404 if the daemon has never written it).

## Texel cadence

Event-driven featurize (capture bursts + quiet stride, decided at ~1.5× piece lead, even subsample to **150**/game by default):

```bash
# Optional: stop workers for a quiet disk window
sudo systemctl stop taikyoku-worker

./deploy/run_texel_cycle.sh \
  --games-dir data/raw/games \
  --out-model models/ab-texel-v1.json

# After a good match vs seed, point MODEL= at the new checkpoint:
#   sudo $EDITOR /etc/taikyoku/worker.env   # MODEL=models/ab-texel-v1.json
sudo systemctl start taikyoku-worker
```

Manual equivalent:

```bash
./target/release/taikyoku_shogi featurize --games-dir data/raw/games --out data/derived/positions
./target/release/taikyoku_shogi texel-fit --features data/derived/positions --out models/ab-texel-v1.json
```

Pull games/models to a laptop anytime with `rsync`/`scp`.
## Throughput sketch

Wall time for one batch ≈ `(BATCH / JOBS) * mean_game_seconds` when CPU-bound. Games/hour ≈ `JOBS * 3600 / mean_game_seconds` (minus overhead). Re-measure after changing depth or model.

## Success checks

- `systemctl stop` finishes the in-flight batch and leaves existing game files intact
- `systemctl start` resumes and increments `games_completed` in `status.json`
- `GET /api/training/status` (via tunnel) matches `status.json`
- Same env + unit on a second VM (e.g. Oracle Free) with only hostname / `SEED_BASE` changes
