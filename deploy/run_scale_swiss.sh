#!/usr/bin/env bash
# Sample ±10% big-param models then run a 5-round Swiss tournament.
#
# Stop anytime:  touch data/run/TOURNEY_STOP   (or Ctrl-C)
# Resume:        ./deploy/run_scale_swiss.sh --resume --run-id ID --skip-gen
#
# Survives SSH logout when started with nohup/disown, or:
#   ./deploy/run_scale_swiss.sh --detach --resume --run-id ID --skip-gen
set -euo pipefail

# Ignore hangup so a bare `... &` is less likely to die on terminal close.
trap '' HUP

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
DETACH=0
PID_FILE="${PID_FILE:-data/run/swiss.pid}"
LOCK_FILE="${LOCK_FILE:-data/run/swiss.lock}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --resume) RESUME=1; shift ;;
    --skip-gen) SKIP_GEN=1; shift ;;
    --detach) DETACH=1; shift ;;
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

if [[ "$DETACH" == "1" ]]; then
  # Re-exec under nohup so logout cannot SIGHUP the tourney wrapper.
  args=()
  [[ "$RESUME" == "1" ]] && args+=(--resume)
  [[ "$SKIP_GEN" == "1" ]] && args+=(--skip-gen)
  [[ -n "$RUN_ID" ]] && args+=(--run-id "$RUN_ID")
  args+=(--jobs "$JOBS" --games-per-pair "$GAMES_PER_PAIR" --swiss-rounds "$SWISS_ROUNDS"
    --depth "$DEPTH" --manifest "$MANIFEST" --outdir "$OUTDIR" --starts "$STARTS"
    --seed "$SEED_MODEL" --sample-out "$SAMPLE_OUT" --n "$N" --rng-seed "$RNG_SEED" --bin "$BIN")
  log="${SWISS_LOG:-data/run/swiss.log}"
  echo "Detaching Swiss → $log (pid file $PID_FILE)"
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

# Single-flight: refuse overlapping Swiss/tourney binaries.
if pgrep -f 'taikyoku_shogi.*tournament' >/dev/null 2>&1; then
  echo "A tournament process is already running:" >&2
  pgrep -af 'taikyoku_shogi.*tournament' >&2 || true
  echo "Stop it first (touch data/run/TOURNEY_STOP) or wait." >&2
  exit 3
fi

exec 9>"$LOCK_FILE"
if ! flock -n 9; then
  echo "Another swiss wrapper holds $LOCK_FILE" >&2
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
  SEED_MODEL="$SEED_MODEL" OUTDIR="$SAMPLE_OUT" N="$N" RNG_SEED="$RNG_SEED" BIN="$BIN" \
    ./deploy/run_scale_sample.sh
  MANIFEST="$SAMPLE_OUT/manifest.json"
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
  RUN_ID="swiss-$(date -u +%Y%m%dT%H%M%SZ)"
fi

if [[ "$RESUME" == "1" && ! -f "$OUTDIR/$RUN_ID/state.json" ]]; then
  echo "Missing state for resume: $OUTDIR/$RUN_ID/state.json" >&2
  exit 2
fi

ARGS=(tournament --manifest "$MANIFEST" --run-id "$RUN_ID" --outdir "$OUTDIR"
  --starts "$STARTS" --depth "$DEPTH" --jobs "$JOBS"
  --format knockout --swiss-rounds "$SWISS_ROUNDS"
  --games-per-pair "$GAMES_PER_PAIR" --seed-base 1)
if [[ "$RESUME" == "1" ]]; then
  ARGS+=(--resume)
fi

echo "Starting knockout run_id=$RUN_ID jobs=$JOBS games-per-pair=$GAMES_PER_PAIR"
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
SUMMARY=""
PROGRESS=""
if [[ -f "$OUTDIR/$RUN_ID/state.json" ]]; then
  PROGRESS="$(python3 - <<PY
import json
from collections import Counter
from pathlib import Path
st=json.loads(Path("$OUTDIR/$RUN_ID/state.json").read_text())
c=Counter(s.get("status","?") for s in st.get("slots",[]))
print(f"slots={len(st.get('slots',[]))} status={dict(c)} swiss_next={st.get('swiss_next_round')}/{st.get('swiss_rounds')}")
PY
)"
fi
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

INCOMPLETE=0
if [[ -f "$OUTDIR/$RUN_ID/state.json" ]]; then
  INCOMPLETE="$(python3 - <<PY
import json
from pathlib import Path
st=json.loads(Path("$OUTDIR/$RUN_ID/state.json").read_text())
fmt=str(st.get("format","")).lower()
# Continuous formats only end on cooperative stop (binary rc).
if fmt in ("swiss", "knockout"):
    print(0)
else:
    unfinished=any(s.get("status") in ("pending","running","aborted") for s in st.get("slots",[]))
    print(1 if unfinished else 0)
PY
)"
fi

if [[ "$RC" -eq 0 && "$INCOMPLETE" == "0" ]]; then
  SUBJECT="swiss done — $RUN_ID"
elif [[ "$INCOMPLETE" == "1" ]]; then
  SUBJECT="swiss incomplete — $RUN_ID (rc=$RC)"
  [[ "$RC" -eq 0 ]] && RC=1
else
  SUBJECT="swiss stopped/failed (rc=$RC) — $RUN_ID"
fi

"$NOTIFY" "$SUBJECT" "${PROGRESS}

${SUMMARY}

${STANDINGS}

Stop/resume:
  touch data/run/TOURNEY_STOP
  ./deploy/run_scale_swiss.sh --detach --resume --run-id $RUN_ID --skip-gen
"

exit "$RC"
