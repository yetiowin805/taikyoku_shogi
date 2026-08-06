#!/usr/bin/env bash
# Daily digest: game counts on the VPS.
# Skips sending if training or strength-test already notified within the last ~20h
# (see data/run/last_notify_activity), so you only get this when nothing else fired.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

NOTIFY="${NOTIFY:-$ROOT/deploy/notify.sh}"
STATUS="${STATUS:-$ROOT/data/run/status.json}"
GAMES_DIR="${GAMES_DIR:-$ROOT/data/raw/games}"
PARTIAL_DIR="${PARTIAL_DIR:-$ROOT/data/raw/games/partial}"
ACTIVITY_STAMP="${ACTIVITY_STAMP:-$ROOT/data/run/last_notify_activity}"
# Seconds: skip daily mail if activity stamp newer than this (default 20h).
SKIP_IF_ACTIVITY_WITHIN="${SKIP_IF_ACTIVITY_WITHIN:-72000}"
FORCE="${FORCE:-0}"

chmod +x "$NOTIFY" 2>/dev/null || true

if [[ "$FORCE" != "1" && -f "$ACTIVITY_STAMP" ]]; then
  now="$(date +%s)"
  # Prefer file mtime
  if stamp_m="$(stat -c %Y "$ACTIVITY_STAMP" 2>/dev/null || stat -f %m "$ACTIVITY_STAMP" 2>/dev/null)"; then
    age=$((now - stamp_m))
    if [[ "$age" -lt "$SKIP_IF_ACTIVITY_WITHIN" ]]; then
      echo "daily digest skipped (activity ${age}s ago < ${SKIP_IF_ACTIVITY_WITHIN}s)"
      exit 0
    fi
  fi
fi

N_GAMES="$(ls -1 "$GAMES_DIR"/*.json 2>/dev/null | wc -l | tr -d ' ')"
N_PARTIAL="$(ls -1 "$PARTIAL_DIR"/*.json 2>/dev/null | wc -l | tr -d ' ')"

WORKER_LINE="(no status.json)"
if [[ -f "$STATUS" ]]; then
  WORKER_LINE="$(python3 - <<PY
import json
s=json.load(open("$STATUS"))
print(
  f"games_completed={s.get('games_completed')} failed={s.get('games_failed')} "
  f"running={s.get('running')} last={s.get('last_game_id')} "
  f"model={((s.get('config') or {}).get('model'))}"
)
PY
)"
fi

DISK="$(df -h "$ROOT" 2>/dev/null | awk 'NR==2{print $4" free on "$6}' || true)"

SUBJECT="daily — ${N_GAMES} games on disk"
BODY="Daily worker digest (no training/match notify in the last ~20h).

Finished game files:  $N_GAMES  ($GAMES_DIR)
Partial games:        $N_PARTIAL  ($PARTIAL_DIR)
Disk:                 $DISK
Worker status:        $WORKER_LINE

Tunnel GUI: ssh -L 3000:127.0.0.1:3000 USER@HOST → http://127.0.0.1:3000
"

"$NOTIFY" "$SUBJECT" "$BODY"
exit 0
