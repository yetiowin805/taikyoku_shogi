#!/usr/bin/env bash
# Notify helper for long VPS jobs. Never fails the caller (exit 0).
# Config: /etc/taikyoku/notify.env or NOTIFY_ENV path.
set -u

ENV_FILE="${NOTIFY_ENV:-/etc/taikyoku/notify.env}"
if [[ -f "$ENV_FILE" ]]; then
  # shellcheck disable=SC1090
  set -a
  # shellcheck source=/dev/null
  source "$ENV_FILE"
  set +a
fi

SUBJECT="${1:-taikyoku notify}"
BODY="${2:-}"
if [[ -z "$BODY" && ! -t 0 ]]; then
  BODY="$(cat)"
fi

HOST="$(hostname -s 2>/dev/null || hostname || echo unknown)"
STAMP="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
FULL_SUBJECT="[taikyoku@$HOST] $SUBJECT"
FULL_BODY="$BODY

-- 
host=$HOST time=$STAMP"

sent=0

if [[ -n "${NTFY_TOPIC:-}" ]]; then
  base="${NTFY_SERVER:-https://ntfy.sh}"
  if curl -fsS -H "Title: $FULL_SUBJECT" -d "$FULL_BODY" "$base/$NTFY_TOPIC" >/dev/null 2>&1; then
    sent=1
  fi
fi

if [[ -n "${NOTIFY_TO:-}" ]]; then
  if command -v msmtp >/dev/null 2>&1; then
    if printf 'To: %s\nSubject: %s\n\n%s\n' "$NOTIFY_TO" "$FULL_SUBJECT" "$FULL_BODY" \
      | msmtp --account="${MSMTP_ACCOUNT:-default}" "$NOTIFY_TO" 2>/dev/null; then
      sent=1
    fi
  elif command -v mail >/dev/null 2>&1; then
    if printf '%s\n' "$FULL_BODY" | mail -s "$FULL_SUBJECT" "$NOTIFY_TO" 2>/dev/null; then
      sent=1
    fi
  elif command -v sendmail >/dev/null 2>&1; then
    if printf 'To: %s\nSubject: %s\n\n%s\n' "$NOTIFY_TO" "$FULL_SUBJECT" "$FULL_BODY" \
      | sendmail -t 2>/dev/null; then
      sent=1
    fi
  fi
fi

if [[ "$sent" -eq 0 ]]; then
  echo "notify: no transport succeeded (set NOTIFY_TO+msmtp or NTFY_TOPIC)" >&2
fi
exit 0
