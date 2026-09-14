#!/usr/bin/env bash
# Prepare only. This script never stops a service or starts a tournament.
set -euo pipefail
trap 'echo "royal-al preparation failed at line $LINENO; tournament did not start" >&2' ERR
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-models/royal-al-grid}"
if [[ $# -gt 1 || -e "$OUT" ]]; then
  echo "Usage: $0 [NEW_OUTPUT_DIRECTORY]; output must not exist; tournament did not start" >&2
  exit 2
fi
cargo build --locked --release --bins
./deploy/freeze_history.sh --id LOGIC_PRE_ROYAL_AL
./deploy/freeze_history.sh --id LOGIC_PRE_LA
target/release/taikyoku_shogi royal-al-grid --out "$OUT"
python3 deploy/check_royal_al_grid.py --manifest "$OUT/manifest.json" > "$OUT/verified.json.tmp"
mv "$OUT/verified.json.tmp" "$OUT/verified.json"
echo "Prepared and verified $OUT/manifest.json. Tournament did not start."
