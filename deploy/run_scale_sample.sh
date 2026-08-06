#!/usr/bin/env bash
# Generate ±10% big-param sample models → models/scale-sample/ + manifest.json
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BIN="${BIN:-$ROOT/target/release/taikyoku_shogi}"
SEED_MODEL="${SEED_MODEL:-models/ab-seed.json}"
OUTDIR="${OUTDIR:-models/scale-sample}"
N="${N:-31}"
RNG_SEED="${RNG_SEED:-1}"

if [[ ! -x "$BIN" ]]; then
  echo "Missing binary: $BIN" >&2
  exit 1
fi
if [[ ! -f "$SEED_MODEL" ]]; then
  echo "Missing seed model: $SEED_MODEL (do not regenerate — copy existing)" >&2
  exit 1
fi

echo "=== scale-sample seed=$SEED_MODEL n=$N rng_seed=$RNG_SEED → $OUTDIR ==="
"$BIN" scale-sample --seed "$SEED_MODEL" --out "$OUTDIR" --n "$N" --rng-seed "$RNG_SEED"
echo "Ready: $OUTDIR/manifest.json"
