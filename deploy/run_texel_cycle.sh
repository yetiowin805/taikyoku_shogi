#!/usr/bin/env bash
# Featurize + texel-fit, then email/ntfy a summary.
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

N_GAMES="$(ls -1 "$GAMES_DIR"/*.json 2>/dev/null | wc -l | tr -d ' ')"
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

if [[ "$RC" -eq 0 ]]; then
  SUBJECT="texel cycle OK (${ELAPSED}s)"
  BODY="featurize+texel-fit succeeded

games_dir=$GAMES_DIR ($N_GAMES files)
features=$FEATURES ($N_FEAT rows)
out_model=$OUT_MODEL
iters=$ITERS
elapsed_sec=$ELAPSED

tail:
$(tail -n 40 "$LOG")"
else
  SUBJECT="texel cycle FAILED (rc=$RC)"
  BODY="featurize+texel-fit failed (exit $RC)

elapsed_sec=$ELAPSED

log:
$(tail -n 80 "$LOG")"
fi

"$NOTIFY" "$SUBJECT" "$BODY"
exit "$RC"
