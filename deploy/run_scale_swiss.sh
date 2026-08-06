#!/usr/bin/env bash
# Sample ±10% big-param models then run a 5-round Swiss tournament.
#
# Stop anytime:  touch data/run/TOURNEY_STOP   (or Ctrl-C)
# Resume:        ./deploy/run_scale_swiss.sh --resume --run-id ID --skip-gen
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BIN="${BIN:-$ROOT/target/release/taikyoku_shogi}"
NOTIFY="${NOTIFY:-$ROOT/deploy/notify.sh}"
JOBS="${JOBS:-$(nproc 2>/dev/null || echo 2)}"
GAMES_PER_PAIR="${GAMES_PER_PAIR:-1}"
SWISS_ROUNDS="${SWISS_ROUNDS:-5}"
DEPTH="${DEPTH:-2}"
STARTS="${STARTS:-light}"
MANIFEST="${MANIFEST:-models/scale-sample/manifest.json}"
OUTDIR="${OUTDIR:-data/raw/tourney}"
SEED_MODEL="${SEED_MODEL:-models/ab-seed.json}"
SAMPLE_OUT="${SAMPLE_OUT:-models/scale-sample}"
N="${N:-31}"
RNG_SEED="${RNG_SEED:-1}"
RUN_ID="${RUN_ID:-}"
RESUME=0
SKIP_GEN=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --resume) RESUME=1; shift ;;
    --skip-gen) SKIP_GEN=1; shift ;;
    --run-id) RUN_ID="$2"; shift 2 ;;
    --jobs) JOBS="$2"; shift 2 ;;
    --games-per-pair) GAMES_PER_PAIR="$2"; shift 2 ;;
    --swiss-rounds) SWISS_ROUNDS="$2"; shift 2 ;;
    --depth) DEPTH="$2"; shift 2 ;;
    --manifest) MANIFEST="$2"; shift 2 ;;
    --outdir) OUTDIR="$2"; shift 2 ;;
    --starts) STARTS="$2"; shift 2 ;;
    --seed) SEED_MODEL="$2"; shift 2 ;;
    --sample-out) SAMPLE_OUT="$2"; shift 2 ;;
    --n) N="$2"; shift 2 ;;
    --rng-seed) RNG_SEED="$2"; shift 2 ;;
    --bin) BIN="$2"; shift 2 ;;
    *) echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

chmod +x deploy/*.sh 2>/dev/null || true
mkdir -p data/run "$OUTDIR"

if [[ ! -x "$BIN" ]]; then
  echo "Missing binary: $BIN (cargo build --release)" >&2
  exit 1
fi

if systemctl is-active --quiet taikyoku-worker 2>/dev/null; then
  echo "Stopping taikyoku-worker (SIGKILL)…"
  sudo systemctl kill -s SIGKILL taikyoku-worker 2>/dev/null || true
  sudo systemctl stop taikyoku-worker 2>/dev/null || true
fi

if [[ "$SKIP_GEN" != "1" && "$RESUME" != "1" ]]; then
  SEED_MODEL="$SEED_MODEL" OUTDIR="$SAMPLE_OUT" N="$N" RNG_SEED="$RNG_SEED" BIN="$BIN" \
    ./deploy/run_scale_sample.sh
  MANIFEST="$SAMPLE_OUT/manifest.json"
fi

if [[ ! -f "$MANIFEST" ]]; then
  echo "Missing manifest: $MANIFEST" >&2
  exit 1
fi

if [[ -z "$RUN_ID" ]]; then
  RUN_ID="swiss-$(date -u +%Y%m%dT%H%M%SZ)"
fi

ARGS=(tournament --manifest "$MANIFEST" --run-id "$RUN_ID" --outdir "$OUTDIR"
  --starts "$STARTS" --depth "$DEPTH" --jobs "$JOBS"
  --format swiss --swiss-rounds "$SWISS_ROUNDS"
  --games-per-pair "$GAMES_PER_PAIR" --seed-base 1)
if [[ "$RESUME" == "1" ]]; then
  ARGS+=(--resume)
fi

echo "Starting Swiss run_id=$RUN_ID rounds=$SWISS_ROUNDS jobs=$JOBS games-per-pair=$GAMES_PER_PAIR"
echo "Stop: touch data/run/TOURNEY_STOP   or Ctrl-C"

set +e
"$BIN" "${ARGS[@]}"
RC=$?
set -e

STANDINGS=""
if [[ -f "$OUTDIR/$RUN_ID/standings.md" ]]; then
  STANDINGS="$(cat "$OUTDIR/$RUN_ID/standings.md")"
fi
SUMMARY=""
if [[ -f "$OUTDIR/$RUN_ID/elo.json" ]]; then
  SUMMARY="$(python3 - <<PY
import json
from pathlib import Path
elo=json.loads(Path("$OUTDIR/$RUN_ID/elo.json").read_text())
rows=sorted(elo.items(), key=lambda kv: -kv[1])
print("run=$RUN_ID")
for i,(k,v) in enumerate(rows,1):
    print(f"{i}. {k}: {v:.1f}")
PY
)"
fi

if [[ "$RC" -eq 0 ]]; then
  SUBJECT="swiss done — $RUN_ID"
else
  SUBJECT="swiss stopped/failed (rc=$RC) — $RUN_ID"
fi

"$NOTIFY" "$SUBJECT" "${SUMMARY}

${STANDINGS}

Stop/resume:
  touch data/run/TOURNEY_STOP
  ./deploy/run_scale_swiss.sh --resume --run-id $RUN_ID --skip-gen
"

exit "$RC"
