#!/usr/bin/env bash
# Optional: notify when games_completed crosses milestones (systemd timer).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STATUS="${STATUS:-$ROOT/data/run/status.json}"
STATE_FILE="${MILESTONE_STATE:-$ROOT/data/run/notify_milestone.txt}"
NOTIFY="${NOTIFY:-$ROOT/deploy/notify.sh}"
STEP="${MILESTONE_STEP:-50}"

chmod +x "$NOTIFY" 2>/dev/null || true

if [[ ! -f "$STATUS" ]]; then
  exit 0
fi

COMPLETED="$(python3 -c "import json;print(json.load(open('$STATUS')).get('games_completed',0))" 2>/dev/null || echo 0)"
LAST=0
if [[ -f "$STATE_FILE" ]]; then
  LAST="$(cat "$STATE_FILE" 2>/dev/null || echo 0)"
fi

# Next milestone strictly above LAST
NEXT=$(( (LAST / STEP + 1) * STEP ))
if [[ "$COMPLETED" -ge "$NEXT" ]]; then
  "$NOTIFY" "games_completed=$COMPLETED" "Worker milestone: $COMPLETED games (step=$STEP).

See data/run/status.json on the VPS."
  echo "$COMPLETED" >"$STATE_FILE"
fi
exit 0
