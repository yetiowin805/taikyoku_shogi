#!/usr/bin/env bash
# Fit several Texel regimens on the same features → models/tourney/ + manifest.json
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BIN="${BIN:-$ROOT/target/release/taikyoku_shogi}"
FEATURES="${FEATURES:-data/derived/positions}"
OUTDIR="${OUTDIR:-models/tourney}"
SEED_MODEL="${SEED_MODEL:-models/ab-seed.json}"

if [[ ! -x "$BIN" ]]; then
  echo "Missing binary: $BIN" >&2
  exit 1
fi
if [[ ! -d "$FEATURES" ]] || [[ -z "$(ls -A "$FEATURES"/*.json 2>/dev/null || true)" ]]; then
  echo "No features in $FEATURES — run featurize first" >&2
  exit 1
fi
if [[ ! -f "$SEED_MODEL" ]]; then
  echo "Missing seed model: $SEED_MODEL" >&2
  exit 1
fi

mkdir -p "$OUTDIR"
cp -f "$SEED_MODEL" "$OUTDIR/seed.json"

run_fit() {
  local id="$1" iters="$2" lr="$3" out="$OUTDIR/${id}.json"
  echo "=== fit $id (iters=$iters lr=$lr) → $out ==="
  "$BIN" texel-fit --features "$FEATURES" --out "$out" --iters "$iters" --lr "$lr" | tee "$OUTDIR/${id}.fit.log"
}

run_fit texel-base 300 0.5
run_fit texel-long 1000 0.5
run_fit texel-hot 300 2.0
run_fit texel-cool 300 0.1

python3 - <<PY
import json, re, pathlib
outdir = pathlib.Path("$OUTDIR")
entrants = [{"id": "seed", "model": str(outdir / "seed.json")}]
for id in ["texel-base", "texel-long", "texel-hot", "texel-cool"]:
    log = (outdir / f"{id}.fit.log").read_text()
    m = re.search(r"max\|Δw\|=([0-9.]+)", log) or re.search(r"max\|\\\\u0394w\|=([0-9.]+)", log)
    # Also match ASCII 'max|Δw|=' from Rust or 'max|Δw|='
    m = re.search(r"max\|.w\|=([0-9.eE+-]+)", log)
    delta = float(m.group(1)) if m else None
    if delta is not None and delta < 1e-4:
        print(f"WARNING: {id} barely moved weights (max|Δw|={delta})")
    entrants.append({"id": id, "model": str(outdir / f"{id}.json")})
manifest = {"entrants": entrants}
path = outdir / "manifest.json"
path.write_text(json.dumps(manifest, indent=2) + "\n")
print(f"Wrote {path} ({len(entrants)} entrants)")
PY

echo "Regimens ready under $OUTDIR"
