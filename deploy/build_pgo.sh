#!/usr/bin/env bash
# Installed tooling builds the requested source; it never publishes binaries.
set -Eeuo pipefail
trap 'echo "PGO build failed during launcher setup; no tournament was started or updated." >&2' ERR
exec python3 "$(dirname "$(realpath "$0")")/pgo_build.py" "$@"
