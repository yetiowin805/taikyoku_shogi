# Cloud continuous Texel workers

Run self-play on a cheap Linux VPS (Hetzner first), accumulate games under `data/raw/games`, then featurize / texel-fit on a quiet window. Same binary and systemd unit work later on a second host (including Oracle Always Free).

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

Incomplete exits (stop file / aborted slots) now fail the tournament binary with a non-zero status so notify says stopped/incomplete rather than “done”.

### Loud-grid continuous Glicko Swiss (preferred)

3×3 material grid (two-movers T ∈ {80,100,150}% × capturers C ∈ {50,100,120}%, FreeKing with capturers; center `T100C100` = seed), then a **continuous** Swiss with Glicko-1 ratings until you stop it. Prefer `--detach` (or the systemd unit) so SSH logout cannot kill the job:

```bash
cargo build --release
# fresh (detach from terminal)
./deploy/run_loud_swiss.sh --detach --jobs "$(nproc)"

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
