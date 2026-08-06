#!/usr/bin/env bash
# Run ab vs ab match (strength test) and notify with scoreboard.
# Usage: deploy/run_match.sh --model-a PATH --model-b PATH [--games N] [--depth N] ...
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BIN="${BIN:-$ROOT/target/release/taikyoku_shogi}"
NOTIFY="${NOTIFY:-$ROOT/deploy/notify.sh}"
ACTIVITY_STAMP="${ACTIVITY_STAMP:-$ROOT/data/run/last_notify_activity}"
MODEL_A=models/ab-seed.json
MODEL_B=models/ab-texel-v1.json
GAMES=16
DEPTH=2
STARTS=light
OUTDIR=data/raw/match_seed_vs_texel_v1
SEED_BASE=0
JOBS="${JOBS:-1}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --model-a) MODEL_A="$2"; shift 2 ;;
    --model-b) MODEL_B="$2"; shift 2 ;;
    --games) GAMES="$2"; shift 2 ;;
    --depth) DEPTH="$2"; shift 2 ;;
    --starts) STARTS="$2"; shift 2 ;;
    --outdir) OUTDIR="$2"; shift 2 ;;
    --seed-base) SEED_BASE="$2"; shift 2 ;;
    --jobs) JOBS="$2"; shift 2 ;;
    --bin) BIN="$2"; shift 2 ;;
    *) echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [[ ! -x "$BIN" ]]; then
  echo "Missing binary: $BIN" >&2
  exit 1
fi
chmod +x "$NOTIFY" 2>/dev/null || true
mkdir -p "$OUTDIR" data/run

START="$(date +%s)"
LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT

set +e
"$BIN" match --a ab --b ab \
  --model-a "$MODEL_A" --model-b "$MODEL_B" \
  --games "$GAMES" --depth "$DEPTH" \
  --starts "$STARTS" --outdir "$OUTDIR" \
  --seed-base "$SEED_BASE" --jobs "$JOBS" \
  2>&1 | tee "$LOG"
RC=${PIPESTATUS[0]}
set -e
END="$(date +%s)"
ELAPSED=$((END - START))

SCORE="$(grep -E 'Match:|scoreboard|A |B |Draw|wins' "$LOG" | tail -n 20 || true)"
N_MATCH_GAMES="$(ls -1 "$OUTDIR"/*.json 2>/dev/null | wc -l | tr -d ' ')"

if [[ "$RC" -eq 0 ]]; then
  SUBJECT="strength test OK — $MODEL_B vs $MODEL_A"
  BODY="Match (strength test) finished.

Control (A):  $MODEL_A
Challenger (B): $MODEL_B
Pairs:        $GAMES
Depth:        $DEPTH
Starts:       $STARTS
Outdir:       $OUTDIR ($N_MATCH_GAMES game files)
Elapsed:      ${ELAPSED}s

Scoreboard / summary:
$SCORE

Log tail:
$(tail -n 40 "$LOG")"
  date -u +%Y-%m-%dT%H:%M:%SZ >"$ACTIVITY_STAMP"
else
  SUBJECT="strength test FAILED (rc=$RC)"
  BODY="Match failed (exit $RC).

model_a=$MODEL_A
model_b=$MODEL_B
elapsed_sec=$ELAPSED

$(tail -n 80 "$LOG")"
fi

"$NOTIFY" "$SUBJECT" "$BODY"
exit "$RC"
