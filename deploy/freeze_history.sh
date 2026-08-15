#!/usr/bin/env bash
# Rebuild pinned logic binaries from models/history/manifest.json.
# Binaries land in models/history/bin/{id} (gitignored). Seeds in
# models/history/models/{id}.json.
#
# Usage: ./deploy/freeze_history.sh [--id LOGIC_H105]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

MANIFEST="${MANIFEST:-models/history/manifest.json}"
BIN_DIR="${BIN_DIR:-models/history/bin}"
MODEL_DIR="${MODEL_DIR:-models/history/models}"
ONLY_ID="${ONLY_ID:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --id) ONLY_ID="$2"; shift 2 ;;
    *) echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [[ ! -f "$MANIFEST" ]]; then
  echo "Missing $MANIFEST" >&2
  exit 1
fi

mkdir -p "$BIN_DIR" "$MODEL_DIR"

splice_think_loop() {
  local dest="$1"
  cp "$ROOT/src/think_loop.rs" "$dest/src/think_loop.rs"
  if ! grep -q 'pub mod think_loop;' "$dest/src/lib.rs"; then
    if grep -q 'pub mod tengu_attack;' "$dest/src/lib.rs"; then
      sed -i '/pub mod tengu_attack;/a pub mod think_loop;' "$dest/src/lib.rs"
    else
      printf '\npub mod think_loop;\n' >>"$dest/src/lib.rs"
    fi
  fi
  if ! grep -q '"think-loop"' "$dest/src/main.rs"; then
    python3 - "$dest/src/main.rs" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1])
text = p.read_text()
if '"think-loop"' in text:
    raise SystemExit(0)
needle = '            "tournament" => {'
arm = '''            "think-loop" => {
                taikyoku_shogi::think_loop::run_think_loop();
            }
            "tournament" => {'''
if needle not in text:
    # Older trees: insert before the default `_` arm in main's match.
    needle = '            _ => {'
    arm = '''            "think-loop" => {
                taikyoku_shogi::think_loop::run_think_loop();
            }
            _ => {'''
if needle not in text:
    raise SystemExit("could not splice think-loop into main.rs")
p.write_text(text.replace(needle, arm, 1))
PY
  fi
}

mapfile -t ENGINES < <(python3 - "$MANIFEST" "$ONLY_ID" <<'PY'
import json, sys
man = json.loads(open(sys.argv[1]).read())
only = sys.argv[2]
for e in man.get("engines", []):
    if only and e.get("id") != only:
        continue
    print(f"{e['id']}\t{e['git']}")
PY
)

if [[ ${#ENGINES[@]} -eq 0 ]]; then
  echo "No logic engines to freeze."
  exit 0
fi

for row in "${ENGINES[@]}"; do
  id="${row%%$'\t'*}"
  rev="${row#*$'\t'}"
  echo "=== freeze $id @ $rev ==="
  git show "${rev}:models/ab-seed.json" >"$MODEL_DIR/${id}.json"
  wt="$ROOT/.history-worktrees/$id"
  rm -rf "$wt"
  git worktree add --detach "$wt" "$rev"
  rustc_ver="$(rustc -V 2>/dev/null || true)"
  set +e
  (
    set -e
    splice_think_loop "$wt"
    cd "$wt"
    cargo build --release
  )
  rc=$?
  set -e
  if [[ $rc -ne 0 ]]; then
    echo "BUILD FAILED for $id ($rev). Leaving weights-only; see worktree $wt" >&2
    printf '%s\n' "build_error rustc=$rustc_ver rev=$rev" >"$BIN_DIR/${id}.build_error"
    git worktree remove --force "$wt" || true
    continue
  fi
  cp "$wt/target/release/taikyoku_shogi" "$BIN_DIR/$id"
  printf '%s\n' "$rustc_ver" "rev=$rev" >"$BIN_DIR/${id}.meta"
  git worktree remove --force "$wt" || true
  echo "Wrote $BIN_DIR/$id"
done
