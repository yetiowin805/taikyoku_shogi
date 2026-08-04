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
6. `pool generate --count 128` (or copy starts from another machine).
7. Install systemd unit + env (below); `systemctl enable --now taikyoku-worker`.
8. Calibrate: one short batch with `--jobs` equal to vCPUs, note wall time → games/hour.

## Install systemd

```bash
sudo mkdir -p /etc/taikyoku
sudo cp deploy/worker.env.example /etc/taikyoku/worker.env
sudo $EDITOR /etc/taikyoku/worker.env

sudo cp deploy/systemd/taikyoku-worker.service /etc/systemd/system/
# Optional weekly starts refresh:
sudo cp deploy/systemd/taikyoku-pool.service deploy/systemd/taikyoku-pool.timer /etc/systemd/system/

sudo systemctl daemon-reload
sudo systemctl enable --now taikyoku-worker
# sudo systemctl enable --now taikyoku-pool.timer
```

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

Count games: `ls data/raw/games/*.json 2>/dev/null | wc -l`

### GUI via SSH tunnel (recommended)

On the VPS (as the `taikyoku` user, after `web` build if you want the UI):

```bash
./target/release/taikyoku_shogi serve 3000
```

On your laptop:

```bash
ssh -L 3000:127.0.0.1:3000 user@vps
# open http://127.0.0.1:3000 — load games from data/raw/games via the GUI
```

### HTTP status without the full GUI

With `serve` running (tunnel as above):

```bash
curl -s http://127.0.0.1:3000/api/training/status | jq .
```

Returns the contents of `data/run/status.json` (404 if the daemon has never written it).

## Texel cadence

Stop workers (or wait for a quiet window) before a heavy fit on the same disk:

```bash
sudo systemctl stop taikyoku-worker
./target/release/taikyoku_shogi featurize --games-dir data/raw/games --out data/derived/positions
./target/release/taikyoku_shogi texel-fit --features data/derived/positions --out models/ab-texel.json
# point MODEL= at the new checkpoint, then:
sudo systemctl start taikyoku-worker
```

Pull games/models to a laptop anytime with `rsync`/`scp`.

## Throughput sketch

Wall time for one batch ≈ `(BATCH / JOBS) * mean_game_seconds` when CPU-bound. Games/hour ≈ `JOBS * 3600 / mean_game_seconds` (minus overhead). Re-measure after changing depth or model.

## Success checks

- `systemctl stop` finishes the in-flight batch and leaves existing game files intact
- `systemctl start` resumes and increments `games_completed` in `status.json`
- `GET /api/training/status` (via tunnel) matches `status.json`
- Same env + unit on a second VM (e.g. Oracle Free) with only hostname / `SEED_BASE` changes
