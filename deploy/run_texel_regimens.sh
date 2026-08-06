#!/usr/bin/env bash
# Fit tourney entrants on the same features → models/tourney/ + manifest.json
#
# Grid (intentionally small — old seed-nudge regimens were ~seed):
#   seed              hand eval (baseline)
#   texel-hot-legacy  prior seed-init additive hot fit (tiny nudge control)
#   fresh-base        seed + log-space, 2500 iters, lr=0.05 (all labels)
#   fresh-hot         seed + log-space, 2500 iters, lr=0.15
#   fresh-long        seed + log-space, 5000 iters, lr=0.05
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BIN="${BIN:-$ROOT/target/release/taikyoku_shogi}"
FEATURES="${FEATURES:-data/derived/positions}"
OUTDIR="${OUTDIR:-models/tourney}"
SEED_MODEL="${SEED_MODEL:-models/ab-seed.json}"
LEGACY_HOT="${LEGACY_HOT:-models/texel-hot.json}"

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

# Keep the old hot fit as a near-seed control if present; else re-fit legacy style.
if [[ -f "$LEGACY_HOT" ]]; then
  echo "=== copy legacy hot $LEGACY_HOT → $OUTDIR/texel-hot-legacy.json ==="
  cp -f "$LEGACY_HOT" "$OUTDIR/texel-hot-legacy.json"
  echo "legacy copy (no refit)" | tee "$OUTDIR/texel-hot-legacy.fit.log"
else
  echo "=== fit texel-hot-legacy (seed init, additive, iters=300 lr=2) ==="
  "$BIN" texel-fit \
    --features "$FEATURES" \
    --out "$OUTDIR/texel-hot-legacy.json" \
    --init seed \
    --iters 300 \
    --lr 2.0 \
    --late-frac 0 \
    --keep-draws \
    --no-log-space \
    --no-lr-scale-k \
    --no-renorm-pawn \
    | tee "$OUTDIR/texel-hot-legacy.fit.log"
fi

run_fresh() {
  local id="$1"
  local iters="$2"
  local lr="$3"
  local out="$OUTDIR/${id}.json"
  echo "=== fit $id (seed log-space iters=$iters lr=$lr) → $out ==="
  "$BIN" texel-fit \
    --features "$FEATURES" \
    --out "$out" \
    --init seed \
    --iters "$iters" \
    --lr "$lr" \
    --late-frac 0 \
    --keep-draws \
    | tee "$OUTDIR/${id}.fit.log"
}

run_fresh fresh-base 2500 0.05
run_fresh fresh-hot 2500 0.15
run_fresh fresh-long 5000 0.05

python3 - <<PY
import json, re, pathlib
outdir = pathlib.Path("$OUTDIR")
ids = ["seed", "texel-hot-legacy", "fresh-base", "fresh-hot", "fresh-long"]
entrants = []
for id in ids:
    path = outdir / f"{id}.json"
    if not path.is_file():
        raise SystemExit(f"missing {path}")
    entry = {"id": id, "model": str(path)}
    log = outdir / f"{id}.fit.log"
    if log.is_file():
        text = log.read_text()
        m = re.search(r"max%Δ=([0-9.eE+-]+)", text) or re.search(r"max%.=([0-9.eE+-]+)", text)
        if m:
            pct = float(m.group(1))
            entry["max_pct_delta"] = pct
            if id.startswith("fresh") and pct < 5.0:
                print(f"WARNING: {id} max%Δ={pct} looks small for a fresh fit")
        m2 = re.search(r"max\|.w\|=([0-9.eE+-]+)", text)
        if m2:
            entry["max_abs_delta"] = float(m2.group(1))
    entrants.append(entry)
manifest = {"entrants": entrants}
path = outdir / "manifest.json"
path.write_text(json.dumps(manifest, indent=2) + "\n")
print(f"Wrote {path} ({len(entrants)} entrants)")
for e in entrants:
    print(f"  - {e['id']}: {e['model']}")
PY

echo "Regimens ready under $OUTDIR"
