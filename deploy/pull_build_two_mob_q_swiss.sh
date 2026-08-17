#!/usr/bin/env bash
# Pull a branch, release-build as the taikyoku user (with cargo on PATH), then
# start a detached two-mob-q-grid Swiss. Defaults: 1s/move soft budget, depth 8.
#
# From root on the VPS (this branch is not on main until merged):
#   cd /opt/taikyoku_shogi && ./deploy/pull_build_two_mob_q_swiss.sh --branch cursor/two-mob-q-swiss
#
# Options (ours first; remainder forwarded to run_two_mob_q_swiss.sh):
#   --jobs N  --time-ms MS  --depth N  --branch main  --skip-pull  --skip-build
#   --no-detach  --no-kill  --resume --run-id ID --skip-gen
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

REPO_USER="${REPO_USER:-taikyoku}"
BRANCH="${BRANCH:-main}"
REMOTE="${REMOTE:-origin}"
TIME_MS="${TIME_MS:-1000}"
DEPTH="${DEPTH:-8}"
JOBS="${JOBS:-$(nproc 2>/dev/null || echo 2)}"
DETACH="${DETACH:-1}"
SKIP_PULL="${SKIP_PULL:-0}"
SKIP_BUILD="${SKIP_BUILD:-0}"
KILL_OLD="${KILL_OLD:-1}"

FORWARD=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-detach) DETACH=0; shift ;;
    --skip-pull) SKIP_PULL=1; shift ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    --no-kill) KILL_OLD=0; shift ;;
    --branch) BRANCH="$2"; shift 2 ;;
    --user) REPO_USER="$2"; shift 2 ;;
    --time-ms) TIME_MS="$2"; shift 2 ;;
    --depth) DEPTH="$2"; shift 2 ;;
    --jobs) JOBS="$2"; shift 2 ;;
    --) shift; FORWARD+=("$@"); break ;;
    *) FORWARD+=("$1"); shift ;;
  esac
done

# Run a bash script string as REPO_USER with cargo/rustup on PATH.
run_as_repo() {
  local script="$1"
  local home
  home="$(getent passwd "$REPO_USER" | cut -d: -f6)"
  if [[ -z "$home" ]]; then
    echo "unknown user: $REPO_USER" >&2
    exit 1
  fi
  local wrapper
  wrapper=$(
    cat <<EOF
set -euo pipefail
cd $(printf '%q' "$ROOT")
export HOME=$(printf '%q' "$home")
if [[ -f "\$HOME/.cargo/env" ]]; then
  # shellcheck disable=SC1091
  source "\$HOME/.cargo/env"
fi
export PATH="\$HOME/.cargo/bin:\${CARGO_HOME:+\$CARGO_HOME/bin}:/usr/local/cargo/bin:\$PATH"
if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found for user $REPO_USER (expected \$HOME/.cargo). Install rustup for that user." >&2
  exit 1
fi
$script
EOF
  )
  if [[ "$(id -un)" == "$REPO_USER" ]]; then
    bash -lc "$wrapper"
  else
    sudo -u "$REPO_USER" -H bash -lc "$wrapper"
  fi
}

mkdir -p "$ROOT/data/run"

if [[ "$KILL_OLD" == "1" ]]; then
  echo "Stopping any running tournament…"
  run_as_repo 'touch data/run/TOURNEY_STOP' || true
  pkill -9 -f 'taikyoku_shogi.*tournament' 2>/dev/null || true
  for _ in 1 2 3 4 5; do
    pgrep -f 'taikyoku_shogi.*tournament' >/dev/null 2>&1 || break
    sleep 1
  done
  run_as_repo 'rm -f data/run/TOURNEY_STOP data/run/two-mob-q-swiss.lock data/run/two-mob-swiss.lock' || true
fi

if [[ "$SKIP_PULL" != "1" ]]; then
  echo "Pulling $REMOTE/$BRANCH as $REPO_USER…"
  run_as_repo "
git fetch $(printf '%q' "$REMOTE")
git checkout $(printf '%q' "$BRANCH")
git pull --ff-only $(printf '%q' "$REMOTE") $(printf '%q' "$BRANCH")
"
fi

if [[ "$SKIP_BUILD" != "1" ]]; then
  echo "cargo build --release as $REPO_USER…"
  run_as_repo 'cargo build --release'
fi

BIN="${BIN:-$ROOT/target/release/taikyoku_shogi}"
if [[ ! -x "$BIN" ]]; then
  echo "Missing binary after build: $BIN" >&2
  exit 1
fi

echo "Freezing historical logic binaries if missing…"
run_as_repo './deploy/freeze_history.sh'

# Build argv for run_two_mob_q_swiss (always as REPO_USER so logs/pid/games aren't root-owned).
swiss_cmd=(./deploy/run_two_mob_q_swiss.sh
  --jobs "$JOBS"
  --depth "$DEPTH"
  --time-ms "$TIME_MS"
  --bin "$BIN")
[[ "$DETACH" == "1" ]] && swiss_cmd+=(--detach)
swiss_cmd+=("${FORWARD[@]+"${FORWARD[@]}"}")

quoted=""
for a in "${swiss_cmd[@]}"; do
  quoted+=" $(printf '%q' "$a")"
done

echo "Starting two-mob-q Swiss (depth=$DEPTH time_ms=$TIME_MS jobs=$JOBS detach=$DETACH)…"
run_as_repo "$quoted"
