# Live tournament maintenance

The running coordinator owns `state.json`. External tools must not edit it while
the coordinator is running. Game claims, game results, and maintenance commands
all commit under the same coordinator mutex.

## Training admission

Run `python3 deploy/training_admission.py CONFIG --watch` as a supervised service.
CONFIG must provide `run` (tournament directory), `out` (training output), and
`admission_models` (explicit list of model directory names). A model is ready only
when its `status.json` says `completed`, including held-out evaluation.

Each finished model is snapshotted, content-verified, validated by the current game
and analysis binaries, and submitted independently. Admission does not wait for
other models, idle CPUs, game completion, or a new bracket. It preserves existing
results and running games. Existing brackets retain their participants; the
champion worker can select the new entrant immediately and future brackets use the
updated pool. Eligibility does not guarantee the next game involves that entrant:
already scheduled pairs and connectivity-based opponent selection still apply.

The service should use `Restart=on-failure`, `RestartSec=30`, and
`StartLimitIntervalSec=0`; there is no overall runtime watchdog. Failures are
persisted per model and printed to stderr. Other ready models are attempted even
if one model fails. Restart retries the same immutable request and cannot duplicate
an entrant. Admission is successful only after a matching applied receipt and a
check of the live entrant ledger, not merely after submitting a request.

## Commands and receipts

`analysis/tasks/requests/ID.json` contains protocol version 1, run ID, and a tagged
command: `admit`, `pause_assignments`, or `resume_assignments`. Publish using
`deploy/tournament_tasks.py` helpers; do not edit an existing request ID. Admission
pins the descriptor and its NNUE dependencies by SHA-256. The authoritative receipt
and state mutation are committed together in `state.json.runtime_control`.
`analysis/tasks/receipts/ID.json` is a repairable acknowledgement. A crash after the
state commit but before acknowledgement is safe to retry. Invalid batches are
rejected atomically; an edited request ID or storage failure stops new assignments
and reports an explicit task-service error. Already running games retain their
normal result-saving path.

The process-identified `ready.json` is published only after startup recovery.
`error.json` indicates a task-service failure; a successful recovery clears it.
Commands run even with all compute workers occupied. This live task interface is
Linux-only; ordinary tournament operation remains available on other platforms.

Pause/resume commands stop only new assignments:

```
python3 deploy/tournament_tasks.py RUN pause --reason 'scheduler maintenance'
python3 deploy/tournament_tasks.py RUN status
python3 deploy/tournament_tasks.py RUN resume
```

A pause persists across coordinator restart. Check `state.json` for running slots
before stopping a coordinator: its existing stop command aborts unfinished games.

## One-time upgrade from the legacy scheduler

Build and validate a replacement coordinator supporting `tournament-task-protocol`.
Stage any replacement admission service's new ExecStart before invoking:

```
python3 deploy/upgrade_tournament_scheduler.py RUN NEW_COORDINATOR \
  --replace-service taikyoku-v5-admission.service \
  --external-service taikyoku-dual-labels.service
```

The upgrader holds `allocation.lock` continuously, stops the obsolete admission
watcher, waits without an elapsed-time cutoff for game leases to drain, and checks
that no slots or child processes remain running. Only leases actually owned by the
specified external service main PIDs are exempt. It then saves backups, replaces
only the coordinator executable in the launcher configuration, verifies the new
live-task service, releases the gate, and starts the replacement watcher. The
rolling game-engine pointer is checked for changes throughout. Status is in
`analysis/scheduler-upgrade.json`; failure exits nonzero. Never force completion
by killing a legitimate game. A failed migration can be retried; after a failure
inspect status and the coordinator service before taking further action.

## Reproduction

```
cargo test training::runtime_tasks
python3 -m unittest discover -s deploy -p 'test_*.py'
cargo build --bin taikyoku_shogi
python3 deploy/check_live_tasks.py target/debug/taikyoku_shogi
```

The end-to-end fixture uses the real coordinator with eight explicitly released
fake game processes. It checks admission while all workers are busy, unchanged
worker PIDs, result retention, pause against reclaims, receipt-loss recovery,
restart while paused, and actual assignment to the new entrant. Its check deadlines
are test failure detection, not limits on production work.
