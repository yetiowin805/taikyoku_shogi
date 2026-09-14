#!/usr/bin/env bash
# Rebuild pinned logic binaries from models/history/manifest.json.
# Binaries land in models/history/bin/{id} (gitignored). Seeds in
# models/history/models/{id}.json.
#
# Usage: ./deploy/freeze_history.sh [--id LOGIC_H105]
set -euo pipefail
trap 'echo "freeze_history failed at line $LINENO; tournament did not start" >&2' ERR

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

ENGINE_ROWS="$(python3 - "$MANIFEST" "$ONLY_ID" <<'PY'
import json, sys
man = json.loads(open(sys.argv[1]).read())
only = sys.argv[2]
for e in man.get("engines", []):
    if only and e.get("id") != only:
        continue
    print(f"{e['id']}\t{e['git']}")
PY
)"
if [[ -z "$ENGINE_ROWS" ]]; then
  echo "No matching logic engines in $MANIFEST (id=$ONLY_ID); tournament did not start" >&2
  exit 1
fi
mapfile -t ENGINES <<< "$ENGINE_ROWS"

for row in "${ENGINES[@]}"; do
  id="${row%%$'\t'*}"
  rev="${row#*$'\t'}"
  echo "=== freeze $id @ $rev ==="
  git show "${rev}:models/ab-seed.json" >"$MODEL_DIR/${id}.json"
  wt="$ROOT/.history-worktrees/$id"
  if [[ -e "$wt" ]]; then
    echo "Existing history worktree $wt; remove it explicitly before rebuilding. Tournament did not start." >&2
    exit 1
  fi
  git worktree add --detach "$wt" "$rev"
  rustc_ver="$(rustc -V)"
  build_target="${HISTORY_TARGET_DIR:-$wt/target}"
  set +e
  (
    set -e
    splice_think_loop "$wt"
    # Transport-only adapter for revisions that already have iteration telemetry.
    # Keep the historical engine's own replay/evaluation/search implementation.
    if [[ -f "$wt/src/bin/analyze_position.rs" ]]; then
      python3 - "$wt/src/bin/analyze_position.rs" <<'PYADAPTER'
import pathlib, sys
p = pathlib.Path(sys.argv[1]); text = p.read_text()
if '--allow-historical' not in text:
    guard = 'if agent.name != "ab" || agent.engine.is_some() {'
    arity = 'if a.len() != 6 {'
    if text.count(guard) != 1 or text.count(arity) != 1:
        raise SystemExit('unrecognized historical analyzer interface; refusing to adapt it')
    text = text.replace(arity, 'let allow_historical = a.len() == 7 && a[6] == "--allow-historical";\n    if a.len() != 6 && !allow_historical {')
    text = text.replace(guard, 'if agent.name != "ab" || (agent.engine.is_some() && !allow_historical) {')
    p.write_text(text)
PYADAPTER
    fi
    cd "$wt"
    cargo build --locked --release --target-dir "$build_target"
  )
  rc=$?
  set -e
  if [[ $rc -ne 0 ]]; then
    echo "BUILD FAILED for $id ($rev). Tournament did not start; see worktree $wt" >&2
    printf '%s\n' "build_error rustc=$rustc_ver rev=$rev" >"$BIN_DIR/${id}.build_error"
    git worktree remove --force "$wt" || true
    exit "$rc"
  fi
  cp "$build_target/release/taikyoku_shogi" "$BIN_DIR/$id"
  engine_sha="$(sha256sum "$BIN_DIR/$id" | cut -d' ' -f1)"
  printf '%s\n' "$rustc_ver" "rev=$rev" "engine_sha256=$engine_sha" >"$BIN_DIR/${id}.meta"
  if [[ -f "$wt/src/bin/analyze_position.rs" ]]; then
    cp "$build_target/release/analyze_position" "$BIN_DIR/${id}.analyze_position"
    analyzer_sha="$(sha256sum "$BIN_DIR/${id}.analyze_position" | cut -d' ' -f1)"
    printf '%s\n' "analyzer_sha256=$analyzer_sha" >>"$BIN_DIR/${id}.meta"
  fi
  git worktree remove --force "$wt" || true
  echo "Wrote $BIN_DIR/$id"
done
