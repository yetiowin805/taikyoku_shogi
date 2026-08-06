#!/usr/bin/env bash
# Quiet eval trajectories over finished games → interesting.md ranking.
#
# Examples:
#   ./deploy/run_eval_trace.sh
#   ./deploy/run_eval_trace.sh --games-dir data/raw/tourney/<run-id> --top 12
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BIN="${BIN:-$ROOT/target/release/taikyoku_shogi}"
GAMES_DIR="${GAMES_DIR:-data/raw/games}"
MODEL="${MODEL:-models/ab-seed.json}"
OUTDIR="${OUTDIR:-data/derived/eval_traces}"
QUIET_STRIDE="${QUIET_STRIDE:-16}"
TOP="${TOP:-30}"
MAX_GAMES="${MAX_GAMES:-}"
SEARCH_DEPTH="${SEARCH_DEPTH:-2}"
SEARCH_STRIDE="${SEARCH_STRIDE:-5}"
NOTIFY="${NOTIFY:-$ROOT/deploy/notify.sh}"
EXTRA=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --games-dir) GAMES_DIR="$2"; shift 2 ;;
    --model) MODEL="$2"; shift 2 ;;
    --out) OUTDIR="$2"; shift 2 ;;
    --quiet-stride) QUIET_STRIDE="$2"; shift 2 ;;
    --top) TOP="$2"; shift 2 ;;
    --max-games) MAX_GAMES="$2"; shift 2 ;;
    --search-depth) SEARCH_DEPTH="$2"; shift 2 ;;
    --search-stride) SEARCH_STRIDE="$2"; shift 2 ;;
    --no-search) EXTRA+=(--no-search); shift ;;
    --bin) BIN="$2"; shift 2 ;;
    *) echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [[ ! -x "$BIN" ]]; then
  echo "Missing binary: $BIN (cargo build --release)" >&2
  exit 1
fi
if [[ ! -d "$GAMES_DIR" ]]; then
  echo "Missing games dir: $GAMES_DIR" >&2
  exit 1
fi

ARGS=(eval-trace --games-dir "$GAMES_DIR" --model "$MODEL" --out "$OUTDIR"
  --quiet-stride "$QUIET_STRIDE" --top "$TOP"
  --search-depth "$SEARCH_DEPTH" --search-stride "$SEARCH_STRIDE")
if [[ -n "$MAX_GAMES" ]]; then
  ARGS+=(--max-games "$MAX_GAMES")
fi
ARGS+=("${EXTRA[@]+"${EXTRA[@]}"}")

echo "=== eval-trace games=$GAMES_DIR model=$MODEL top=$TOP search=${SEARCH_DEPTH}@${SEARCH_STRIDE} → $OUTDIR ==="
"$BIN" "${ARGS[@]}"

if [[ -x "$NOTIFY" ]] && [[ -f "$OUTDIR/interesting.md" ]]; then
  HEAD="$(head -n 25 "$OUTDIR/interesting.md")"
  "$NOTIFY" "eval-trace done — $OUTDIR" "$HEAD" || true
fi

echo "Read $OUTDIR/interesting.md"
