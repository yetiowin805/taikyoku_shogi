#!/usr/bin/env bash
# Build in isolation; publication is the final step and never restarts workers.
set -Eeuo pipefail
if [[ $# -lt 2 || "$2" != "--compatible-speedup" ]]; then
  echo "usage: $0 RUN_DIR --compatible-speedup [GIT_REF (default origin/main)]" >&2
  exit 2
fi
repo=$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)
run=$(realpath "$1")
ref=${3:-origin/main}
scratch=$(mktemp -d)
cleanup() {
  git -C "$repo" worktree remove --force "$scratch/source" >/dev/null 2>&1 || true
  rm -rf "$scratch"
}
trap cleanup EXIT
trap 'echo "Engine update failed at line $LINENO; inspect update/status output. No tournament restart was requested." >&2' ERR
git -C "$repo" fetch origin
revision=$(git -C "$repo" rev-parse --verify --end-of-options "$ref^{commit}")
git -C "$repo" worktree add --detach "$scratch/source" "$revision"
(
  cd "$scratch/source"
  CARGO_TARGET_DIR="$scratch/target" CARGO_BUILD_JOBS=1 nice -n 19 cargo build --locked --release --bin taikyoku_shogi --bin analyze_position
)
# Use the installed coordinator tooling, not arbitrary deployment code from REF.
python3 "$repo/deploy/tourney_analysis.py" update-engine --run-dir "$run" \
  --engine "$scratch/target/release/taikyoku_shogi" \
  --analyzer "$scratch/target/release/analyze_position" \
  --revision "$revision" --compatible-speedup
