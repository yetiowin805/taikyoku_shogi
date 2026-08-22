#!/usr/bin/env bash
# Fit the mix-tournament top 11, each from its own chassis as --init.
#
# Same features (mix games). Texel only trains piece values; PST / tropism /
# two_mover_mob_k stay on the parent. After the 11 runs, compare_top11_texel.py
# writes a /Pawn %Δ table so we can see if the moves look reasonable before
# transplanting large pieces onto twins.
#
# Usage (on the VPS, after the mix knockout):
#   ./deploy/run_top11_texel_fits.sh \
#     --games-dir data/raw/tourney/top4-mix-swiss-20260820T061654Z
#
# Already featurized:
#   ./deploy/run_top11_texel_fits.sh --skip-featurize
#
# Re-print the table:
#   ./deploy/run_top11_texel_fits.sh --compare-only
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BIN="${BIN:-$ROOT/target/release/taikyoku_shogi}"
COMPARE="${COMPARE:-$ROOT/deploy/compare_top11_texel.py}"
FEATURES="${FEATURES:-data/derived/top11-texel}"
GRID="${GRID:-models/top4-mix-grid}"
OUTDIR="${OUTDIR:-models/top11-texel}"
SEED_MODEL="${SEED_MODEL:-models/ab-seed.json}"
GAMES_DIR="${GAMES_DIR:-}"
ITERS="${ITERS:-2500}"
LR="${LR:-0.05}"
SKIP_FEATURIZE="${SKIP_FEATURIZE:-0}"
SKIP_GRID="${SKIP_GRID:-0}"
COMPARE_ONLY="${COMPARE_ONLY:-0}"
RENORM_PAWN="${RENORM_PAWN:-1}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --games-dir) GAMES_DIR="$2"; shift 2 ;;
    --features) FEATURES="$2"; shift 2 ;;
    --grid) GRID="$2"; shift 2 ;;
    --out) OUTDIR="$2"; shift 2 ;;
    --seed) SEED_MODEL="$2"; shift 2 ;;
    --iters) ITERS="$2"; shift 2 ;;
    --lr) LR="$2"; shift 2 ;;
    --bin) BIN="$2"; shift 2 ;;
    --skip-featurize) SKIP_FEATURIZE=1; shift ;;
    --skip-grid) SKIP_GRID=1; shift ;;
    --compare-only) COMPARE_ONLY=1; shift ;;
    --no-renorm-pawn) RENORM_PAWN=0; shift ;;
    *) echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

mapfile -t IDS < <(python3 "$COMPARE" --list-ids)

latest_mix() {
  ls -1dt data/raw/tourney/top4-mix-swiss-* 2>/dev/null | head -1 || true
}

# Tourney dirs also have state.json / ratings.json. Featurize those as games.
stage_games() {
  local src="$1"
  local dest="$2"
  mkdir -p "$dest"
  find "$dest" -maxdepth 1 -name '*.json' -delete
  local slots=()
  shopt -s nullglob
  slots=("$src"/slot*.json)
  shopt -u nullglob
  if [[ ${#slots[@]} -gt 0 ]]; then
    ln -s "${slots[@]}" "$dest/"
    echo "staged ${#slots[@]} slot games from $src → $dest"
    return
  fi
  local copied=0
  shopt -s nullglob
  for f in "$src"/*.json; do
    local base
    base="$(basename "$f")"
    case "$base" in
      state.json|ratings.json|elo.json|manifest.json|samples.json|compare.json)
        continue
        ;;
    esac
    ln -s "$f" "$dest/"
    copied=$((copied + 1))
  done
  shopt -u nullglob
  if [[ "$copied" -eq 0 ]]; then
    echo "No game JSON in $src" >&2
    exit 1
  fi
  echo "staged $copied games from $src → $dest"
}

if [[ "$COMPARE_ONLY" == "1" ]]; then
  python3 "$COMPARE" --grid "$GRID" --fits "$OUTDIR"
  exit 0
fi

if [[ ! -x "$BIN" ]]; then
  echo "Missing binary: $BIN (cargo build --release)" >&2
  exit 1
fi
if [[ ! -f "$SEED_MODEL" ]]; then
  echo "Missing seed model: $SEED_MODEL" >&2
  exit 1
fi

mkdir -p "$OUTDIR" data/run

if [[ "$SKIP_GRID" != "1" ]]; then
  need_grid=0
  for id in "${IDS[@]}"; do
    if [[ ! -f "$GRID/${id}.json" ]]; then
      need_grid=1
      break
    fi
  done
  if [[ "$need_grid" == "1" ]]; then
    echo "=== top4-mix-grid → $GRID ==="
    "$BIN" top4-mix-grid --seed "$SEED_MODEL" --out "$GRID"
  else
    echo "=== grid already has all 11 chassis ($GRID) ==="
  fi
fi

for id in "${IDS[@]}"; do
  if [[ ! -f "$GRID/${id}.json" ]]; then
    echo "Missing chassis $GRID/${id}.json (run without --skip-grid)" >&2
    exit 1
  fi
done

count_json() {
  local dir="$1"
  local n=0
  if [[ -d "$dir" ]]; then
    shopt -s nullglob
    local files=("$dir"/*.json)
    n=${#files[@]}
    shopt -u nullglob
  fi
  echo "$n"
}

if [[ "$SKIP_FEATURIZE" != "1" ]]; then
  if [[ -z "$GAMES_DIR" ]]; then
    GAMES_DIR="$(latest_mix)"
  fi
  if [[ -z "$GAMES_DIR" || ! -d "$GAMES_DIR" ]]; then
    echo "Need --games-dir (mix tourney run) or --skip-featurize with existing $FEATURES" >&2
    exit 1
  fi
  STAGE="$OUTDIR/games-stage"
  echo "=== featurize $GAMES_DIR → $FEATURES ==="
  stage_games "$GAMES_DIR" "$STAGE"
  "$BIN" featurize --games-dir "$STAGE" --out "$FEATURES"
else
  n_feat="$(count_json "$FEATURES")"
  if [[ "$n_feat" -eq 0 ]]; then
    echo "No features in $FEATURES — drop --skip-featurize or pass --games-dir" >&2
    exit 1
  fi
  echo "=== skip featurize ($n_feat rows in $FEATURES) ==="
fi

renorm_flag=()
if [[ "$RENORM_PAWN" != "1" ]]; then
  renorm_flag+=(--no-renorm-pawn)
fi

for id in "${IDS[@]}"; do
  init="$GRID/${id}.json"
  out="$OUTDIR/${id}-texel.json"
  log="$OUTDIR/${id}-texel.fit.log"
  echo "=== fit $id (init=$init iters=$ITERS lr=$LR) → $out ==="
  "$BIN" texel-fit \
    --features "$FEATURES" \
    --out "$out" \
    --init "$init" \
    --iters "$ITERS" \
    --lr "$LR" \
    --late-frac 0 \
    --keep-draws \
    "${renorm_flag[@]}" \
    | tee "$log"
  python3 - "$out" "$id" <<'PY'
import json, sys
path, name = sys.argv[1], sys.argv[2]
cp = json.loads(open(path).read())
cp["name"] = f"{name}-texel"
open(path, "w").write(json.dumps(cp, indent=2) + "\n")
PY
done

echo "=== compare /Pawn deltas ==="
python3 "$COMPARE" --grid "$GRID" --fits "$OUTDIR"
echo "Fits + table under $OUTDIR"
