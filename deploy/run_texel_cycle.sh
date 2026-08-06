#!/usr/bin/env bash
# Featurize + texel-fit, then email/ntfy a detailed summary.
# Usage: deploy/run_texel_cycle.sh [--games-dir DIR] [--out-model PATH] [--iters N]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

GAMES_DIR=data/raw/games
OUT_MODEL=models/ab-texel-v1.json
FEATURES=data/derived/positions
ITERS=50
BIN="${BIN:-$ROOT/target/release/taikyoku_shogi}"
NOTIFY="${NOTIFY:-$ROOT/deploy/notify.sh}"
ACTIVITY_STAMP="${ACTIVITY_STAMP:-$ROOT/data/run/last_notify_activity}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --games-dir) GAMES_DIR="$2"; shift 2 ;;
    --out-model) OUT_MODEL="$2"; shift 2 ;;
    --features) FEATURES="$2"; shift 2 ;;
    --iters) ITERS="$2"; shift 2 ;;
    --bin) BIN="$2"; shift 2 ;;
    *) echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [[ ! -x "$BIN" ]]; then
  echo "Missing binary: $BIN (cargo build --release first)" >&2
  exit 1
fi
chmod +x "$NOTIFY" 2>/dev/null || true
mkdir -p data/run

N_GAMES="$(ls -1 "$GAMES_DIR"/*.json 2>/dev/null | wc -l | tr -d ' ')"
# Rough mean move count from a sample of game JSONs (fast).
MEAN_MOVES="$(python3 - <<PY
import json, glob, statistics
paths = sorted(glob.glob("$GAMES_DIR/*.json"))
if not paths:
    print("n/a")
else:
    sample = paths if len(paths) <= 40 else paths[:: max(1, len(paths)//40)][:40]
    lens = []
    for p in sample:
        try:
            g = json.load(open(p))
            lens.append(len(g.get("moves") or []))
        except Exception:
            pass
    print(f"{statistics.mean(lens):.0f}" if lens else "n/a")
PY
)"

START="$(date +%s)"
LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT

set +e
{
  echo "=== featurize ($N_GAMES games) ==="
  "$BIN" featurize --games-dir "$GAMES_DIR" --out "$FEATURES"
  echo "=== texel-fit → $OUT_MODEL ==="
  "$BIN" texel-fit --features "$FEATURES" --out "$OUT_MODEL" --iters "$ITERS"
} 2>&1 | tee "$LOG"
RC=${PIPESTATUS[0]}
set -e

END="$(date +%s)"
ELAPSED=$((END - START))
N_FEAT="$(ls -1 "$FEATURES"/*.json 2>/dev/null | wc -l | tr -d ' ')"
AVG_POS="n/a"
if [[ "$N_GAMES" =~ ^[0-9]+$ && "$N_GAMES" -gt 0 && "$N_FEAT" =~ ^[0-9]+$ ]]; then
  AVG_POS="$(python3 -c "print(round($N_FEAT / $N_GAMES, 1))")"
fi
LOSS_LINE="$(grep -E 'Wrote .*mean CE loss' "$LOG" | tail -n 1 || true)"

if [[ "$RC" -eq 0 ]]; then
  SUBJECT="training OK — ${N_GAMES} games → ${N_FEAT} positions"
  BODY="Texel training finished successfully.

Games:          $N_GAMES  ($GAMES_DIR)
Mean moves:     ~$MEAN_MOVES (sample)
Positions:      $N_FEAT  ($FEATURES)
Avg pos/game:   $AVG_POS
Out model:      $OUT_MODEL
Iters:          $ITERS
Elapsed:        ${ELAPSED}s
$LOSS_LINE

Log tail:
$(tail -n 30 "$LOG")"
  date -u +%Y-%m-%dT%H:%M:%SZ >"$ACTIVITY_STAMP"
else
  SUBJECT="training FAILED (rc=$RC)"
  BODY="Texel training failed (exit $RC).

Games attempted from: $GAMES_DIR ($N_GAMES files)
Elapsed: ${ELAPSED}s

Log:
$(tail -n 80 "$LOG")"
fi

"$NOTIFY" "$SUBJECT" "$BODY"
exit "$RC"
