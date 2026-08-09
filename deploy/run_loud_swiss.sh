#!/usr/bin/env bash
# Build the 3×3 HookMover × other-two-mover material grid, then run a continuous Glicko Swiss.
#
# Stop anytime:  touch data/run/TOURNEY_STOP   (or Ctrl-C)
# Resume:        ./deploy/run_loud_swiss.sh --detach --resume --run-id ID --skip-gen
#
# Survives SSH logout when started with:
#   ./deploy/run_loud_swiss.sh --detach
set -euo pipefail

# Ignore hangup so a bare `... &` is less likely to die on terminal close.
trap '' HUP

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BIN="${BIN:-$ROOT/target/release/taikyoku_shogi}"
NOTIFY="${NOTIFY:-$ROOT/deploy/notify.sh}"
JOBS="${JOBS:-$(nproc 2>/dev/null || echo 2)}"
DEPTH="${DEPTH:-2}"
# Soft AB budget (ms). Empty = fixed depth only. When set, omit DEPTH to default ceiling 8 in CLI.
TIME_MS="${TIME_MS:-}"
MANIFEST="${MANIFEST:-models/loud-grid/manifest.json}"
OUTDIR="${OUTDIR:-data/raw/tourney}"
SEED_MODEL="${SEED_MODEL:-models/ab-seed.json}"
GRID_OUT="${GRID_OUT:-models/loud-grid}"
RUN_ID="${RUN_ID:-}"
# Env (systemd) or CLI flags may set these.
RESUME="${RESUME:-0}"
SKIP_GEN="${SKIP_GEN:-0}"
DETACH="${DETACH:-0}"
PID_FILE="${PID_FILE:-data/run/loud-swiss.pid}"
LOCK_FILE="${LOCK_FILE:-data/run/loud-swiss.lock}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --resume) RESUME=1; shift ;;
    --skip-gen) SKIP_GEN=1; shift ;;
    --detach) DETACH=1; shift ;;
    --run-id) RUN_ID="$2"; shift 2 ;;
    --jobs) JOBS="$2"; shift 2 ;;
    --depth) DEPTH="$2"; shift 2 ;;
    --time-ms) TIME_MS="$2"; shift 2 ;;
    --manifest) MANIFEST="$2"; shift 2 ;;
    --outdir) OUTDIR="$2"; shift 2 ;;
    --seed) SEED_MODEL="$2"; shift 2 ;;
    --grid-out) GRID_OUT="$2"; shift 2 ;;
    --bin) BIN="$2"; shift 2 ;;
    *) echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

chmod +x deploy/*.sh 2>/dev/null || true
mkdir -p data/run "$OUTDIR"

if [[ "$DETACH" == "1" ]]; then
  args=()
  [[ "$RESUME" == "1" ]] && args+=(--resume)
  [[ "$SKIP_GEN" == "1" ]] && args+=(--skip-gen)
  [[ -n "$RUN_ID" ]] && args+=(--run-id "$RUN_ID")
  args+=(--jobs "$JOBS" --depth "$DEPTH" --manifest "$MANIFEST" --outdir "$OUTDIR"
    --seed "$SEED_MODEL" --grid-out "$GRID_OUT" --bin "$BIN")
  [[ -n "$TIME_MS" ]] && args+=(--time-ms "$TIME_MS")
  log="${LOUD_SWISS_LOG:-data/run/loud-swiss.log}"
  echo "Detaching loud Swiss → $log (pid file $PID_FILE)"
  nohup "$0" "${args[@]}" >>"$log" 2>&1 &
  echo $! >"$PID_FILE"
  disown $! 2>/dev/null || true
  sleep 1
  if ! kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
    echo "Detach failed — see $log" >&2
    exit 1
  fi
  echo "Running pid=$(cat "$PID_FILE"). Check: pgrep -af tournament && tail -f $log"
  exit 0
fi

if [[ ! -x "$BIN" ]]; then
  echo "Missing binary: $BIN (cargo build --release)" >&2
  exit 1
fi

# Single-flight: refuse overlapping tournament binaries.
if pgrep -f 'taikyoku_shogi.*tournament' >/dev/null 2>&1; then
  echo "A tournament process is already running:" >&2
  pgrep -af 'taikyoku_shogi.*tournament' >&2 || true
  echo "Stop it first (touch data/run/TOURNEY_STOP) or wait." >&2
  exit 3
fi

exec 9>"$LOCK_FILE"
if ! flock -n 9; then
  echo "Another loud-swiss wrapper holds $LOCK_FILE" >&2
  exit 3
fi
echo $$ >"$PID_FILE"
cleanup_pid() { rm -f "$PID_FILE"; }
trap cleanup_pid EXIT

if systemctl is-active --quiet taikyoku-worker 2>/dev/null; then
  echo "Stopping taikyoku-worker (SIGKILL)…"
  sudo systemctl kill -s SIGKILL taikyoku-worker 2>/dev/null || true
  sudo systemctl stop taikyoku-worker 2>/dev/null || true
fi

if [[ "$SKIP_GEN" != "1" && "$RESUME" != "1" ]]; then
  "$BIN" loud-grid --seed "$SEED_MODEL" --out "$GRID_OUT"
  MANIFEST="$GRID_OUT/manifest.json"
fi

if [[ ! -f "$MANIFEST" ]]; then
  echo "Missing manifest: $MANIFEST" >&2
  exit 1
fi

if [[ -z "$RUN_ID" ]]; then
  if [[ "$RESUME" == "1" ]]; then
    echo "--resume requires --run-id" >&2
    exit 2
  fi
  RUN_ID="loud-swiss-$(date -u +%Y%m%dT%H%M%SZ)"
fi

if [[ "$RESUME" == "1" && ! -f "$OUTDIR/$RUN_ID/state.json" ]]; then
  echo "Missing state for resume: $OUTDIR/$RUN_ID/state.json" >&2
  exit 2
fi

ARGS=(tournament --manifest "$MANIFEST" --run-id "$RUN_ID" --outdir "$OUTDIR"
  --depth "$DEPTH" --jobs "$JOBS" --format swiss --seed-base 1)
if [[ -n "$TIME_MS" ]]; then
  ARGS+=(--time-ms "$TIME_MS")
fi
if [[ "$RESUME" == "1" ]]; then
  ARGS+=(--resume)
fi

echo "Starting continuous Glicko Swiss run_id=$RUN_ID jobs=$JOBS depth=$DEPTH${TIME_MS:+ time_ms=$TIME_MS}"
echo "Stop: touch data/run/TOURNEY_STOP   or Ctrl-C"
echo "Detach-safe launch: $0 --detach --resume --run-id $RUN_ID --skip-gen"

set +e
"$BIN" "${ARGS[@]}"
RC=$?
set -e

STANDINGS=""
if [[ -f "$OUTDIR/$RUN_ID/standings.md" ]]; then
  STANDINGS="$(cat "$OUTDIR/$RUN_ID/standings.md")"
fi
PROGRESS=""
SUMMARY=""
if [[ -f "$OUTDIR/$RUN_ID/state.json" ]]; then
  PROGRESS="$(python3 - <<PY
import json
from collections import Counter
from pathlib import Path
st=json.loads(Path("$OUTDIR/$RUN_ID/state.json").read_text())
c=Counter(s.get("status","?") for s in st.get("slots",[]))
print(f"slots={len(st.get('slots',[]))} status={dict(c)} format={st.get('format')}")
PY
)"
fi
if [[ -f "$OUTDIR/$RUN_ID/ratings.json" ]]; then
  SUMMARY="$(python3 - <<PY
import json
from pathlib import Path
ratings=json.loads(Path("$OUTDIR/$RUN_ID/ratings.json").read_text())
rows=sorted(ratings.items(), key=lambda kv: -kv[1].get("r", 0))
print("run=$RUN_ID")
for i,(k,v) in enumerate(rows,1):
    print(f"{i}. {k}: {v.get('r',0):.1f}±{v.get('rd',0):.1f}")
PY
)"
elif [[ -f "$OUTDIR/$RUN_ID/elo.json" ]]; then
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

# Continuous Swiss: stop (rc=0) is the normal end; non-zero is failure.
if [[ "$RC" -eq 0 ]]; then
  SUBJECT="loud-swiss stopped — $RUN_ID"
else
  SUBJECT="loud-swiss failed (rc=$RC) — $RUN_ID"
fi

"$NOTIFY" "$SUBJECT" "${PROGRESS}

${SUMMARY}

${STANDINGS}

Stop/resume:
  touch data/run/TOURNEY_STOP
  ./deploy/run_loud_swiss.sh --detach --resume --run-id $RUN_ID --skip-gen
"

exit "$RC"
