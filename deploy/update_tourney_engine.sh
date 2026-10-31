#!/usr/bin/env bash
# Shared native/PGO preparation. Only the final publication selects future games.
set -Eeuo pipefail
exec python3 "$(dirname "$0")/update_tourney_engine.py" "$@"
