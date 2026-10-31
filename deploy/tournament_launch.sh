#!/usr/bin/env bash
# Shared preflight diagnostics and detached-start acknowledgement. Source this
# after entering the repository; experiment wrappers retain their own profiles.

TAIKYOKU_LAUNCH_STEP=preflight

tourney_abort_child() {
  if [[ -n "${TAIKYOKU_LAUNCH_PID:-}" ]]; then
    pkill -KILL -P "$TAIKYOKU_LAUNCH_PID" 2>/dev/null || true
    kill -KILL "$TAIKYOKU_LAUNCH_PID" 2>/dev/null || true
    wait "$TAIKYOKU_LAUNCH_PID" 2>/dev/null || true
    TAIKYOKU_LAUNCH_PID=""
  fi
}

tourney_launch_exit() {
  local rc="$1"
  if [[ "$rc" -ne 0 ]]; then
    if [[ -n "${TAIKYOKU_TOURNAMENT_READY_FILE:-}" && -s "$TAIKYOKU_TOURNAMENT_READY_FILE" ]]; then
      echo "$TAIKYOKU_LAUNCH_STEP failed (rc=$rc); tournament exited after startup" >&2
    else
      echo "$TAIKYOKU_LAUNCH_STEP failed (rc=$rc); tournament did not start" >&2
    fi
    tourney_abort_child
  fi
  if [[ -n "${PID_FILE:-}" && -f "$PID_FILE" ]]; then
    local recorded_pid
    recorded_pid="$(cat "$PID_FILE")"
    if [[ "$recorded_pid" == "$BASHPID" ]]; then rm -f "$PID_FILE"; fi
  fi
  if [[ "${TAIKYOKU_DETACHED_OK:-0}" != 1 && -n "${TAIKYOKU_TOURNAMENT_READY_FILE:-}" ]]; then
    rm -f "$TAIKYOKU_TOURNAMENT_READY_FILE"
  fi
  return "$rc"
}
trap 'tourney_launch_exit "$?"' EXIT

tourney_prepare_start() {
  TAIKYOKU_LAUNCH_STEP="startup files"
  if [[ ! "${TOURNEY_START_TIMEOUT_SECONDS:-60}" =~ ^[1-9][0-9]*$ ]]; then
    echo "TOURNEY_START_TIMEOUT_SECONDS must be a positive integer" >&2
    return 2
  fi
  if [[ -e "$PID_FILE" && ! -f "$PID_FILE" ]]; then
    echo "PID destination is not a regular file: $PID_FILE" >&2
    return 1
  fi
  if [[ "${TAIKYOKU_LAUNCH_CHILD:-0}" != 1 ]]; then
    TAIKYOKU_TOURNAMENT_READY_FILE="$(mktemp "$ROOT/data/run/.tournament-ready.XXXXXX")"
    export TAIKYOKU_TOURNAMENT_READY_FILE
  fi
}

# Publish this wrapper's PID only once its engine has acknowledged readiness.
# Foreground and detached launches use the same publication/lifetime rules.
tourney_run() {
  "$@" &
  TAIKYOKU_LAUNCH_PID=$!
  local deadline=$((SECONDS + ${TOURNEY_START_TIMEOUT_SECONDS:-60}))
  local marker engine_pid started=0
  while kill -0 "$TAIKYOKU_LAUNCH_PID" 2>/dev/null; do
    if read -r marker engine_pid <"$TAIKYOKU_TOURNAMENT_READY_FILE" &&
        [[ "$marker" == ready && "$engine_pid" == "$TAIKYOKU_LAUNCH_PID" ]]; then
      if ! printf '%s\n' "$BASHPID" >"$PID_FILE"; then
        echo "Cannot publish tournament PID: $PID_FILE" >&2
        tourney_abort_child
        return 1
      fi
      started=1
      break
    fi
    if (( SECONDS >= deadline )); then
      echo "Tournament readiness timed out; tournament did not start" >&2
      tourney_abort_child
      return 1
    fi
    sleep 0.05
  done
  local rc=0
  wait "$TAIKYOKU_LAUNCH_PID" || rc=$?
  # A finite foreground run may finish between polls. A completed ready marker
  # is still valid, but must never leave a PID claiming that process is running.
  if read -r marker engine_pid <"$TAIKYOKU_TOURNAMENT_READY_FILE" &&
      [[ "$marker" == ready && "$engine_pid" == "$TAIKYOKU_LAUNCH_PID" ]]; then
    started=1
  fi
  TAIKYOKU_LAUNCH_PID=""
  if [[ "$rc" -eq 0 && "$started" -eq 0 ]]; then
    echo "Engine exited without readiness; tournament did not start" >&2
    return 1
  fi
  return "$rc"
}

tourney_detach() {
  local log="$1"
  shift
  local timeout="${TOURNEY_START_TIMEOUT_SECONDS:-60}"
  TAIKYOKU_LAUNCH_STEP="tournament startup"
  # FD 9 retains the parent's preflight flock. The child skips preflight and
  # reuses the fully resolved arguments, including the generated manifest.
  nohup env DETACH=0 TAIKYOKU_LAUNCH_CHILD=1 "$@" </dev/null >>"$log" 2>&1 &
  TAIKYOKU_LAUNCH_PID=$!
  local deadline=$((SECONDS + timeout))
  local marker engine_pid recorded_pid
  while kill -0 "$TAIKYOKU_LAUNCH_PID" 2>/dev/null; do
    if read -r marker engine_pid <"$TAIKYOKU_TOURNAMENT_READY_FILE" &&
        [[ "$marker" == ready && "$engine_pid" =~ ^[0-9]+$ ]]; then
      if ! kill -0 "$engine_pid" 2>/dev/null; then break; fi
      if [[ -f "$PID_FILE" ]]; then
        read -r recorded_pid <"$PID_FILE" || recorded_pid=""
      else
        recorded_pid=""
      fi
      if [[ "$recorded_pid" == "$TAIKYOKU_LAUNCH_PID" ]]; then
        TAIKYOKU_DETACHED_OK=1
        disown "$TAIKYOKU_LAUNCH_PID" 2>/dev/null || true
        echo "Running pid=$TAIKYOKU_LAUNCH_PID (engine=$engine_pid); log=$log"
        return 0
      fi
    fi
    if (( SECONDS >= deadline )); then
      echo "Tournament readiness timed out after ${timeout}s; see $log" >&2
      return 1
    fi
    sleep 0.05
  done
  local rc=0
  wait "$TAIKYOKU_LAUNCH_PID" || rc=$?
  TAIKYOKU_LAUNCH_PID=""
  echo "Tournament exited before readiness (rc=$rc); see $log" >&2
  tail -n 20 "$log" >&2 || true
  return 1
}
