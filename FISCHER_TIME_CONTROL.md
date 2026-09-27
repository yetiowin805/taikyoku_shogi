# Tournament Fischer clocks

Use `--initial-time-ms 900000 --increment-ms 5000` for **15 minutes plus
5 seconds per move**, separately for each player. This is Fischer increment,
not byoyomi. The increment is credited after an on-time legal move.

## Search policy

The sustainable move budget is five seconds, including move generation overhead.
After each completed depth, predict the next depth as twice the duration of the
last iteration. Complete the iterations predicted to fit the sustainable budget,
then try **at most one additional depth** if its estimated cost fits the remaining
clock minus a one-second reserve. Return immediately after that extra depth,
even if the prediction was conservative. A low clock can therefore stop earlier.

The actual hard search deadline is the remaining clock minus one second. On
expiry, use the last completed iteration, or a legal fallback if none completed.
An unexpectedly slow iteration can spend more than its estimate, but cannot
trigger another extra depth. Deadline checks are cooperative: a long indivisible
operation or OS scheduling delay can overshoot; an actual flag fall is a loss.
If already below the reserve, submit a legal fallback without starting a search.
The default Fischer depth ceiling is 64; an explicit `--depth` remains a ceiling.
Neither the increment nor a target depth guarantees that depth five will finish.

Fixed `--time-ms` searches and analyzer searches keep their existing policy.
Do not combine `--time-ms` with Fischer flags. Historical external think-loop
binaries are rejected because their protocol cannot accept a per-move clock.

## Start and resume

Both the engine's `tournament` command and `deploy/tourney_analysis.py` accept
the two flags. For example, after building the new engine and analyzer:

```sh
python3 deploy/tourney_analysis.py resume --run-dir data/raw/tourney/RUN \
  --initial-time-ms 900000 --increment-ms 5000
```

Use the existing run's normal engine/analyzer and sidecar options as needed.
Omitting the flags on subsequent resumes preserves the saved time control.
An explicit transition preserves all completed slots, brackets and ratings;
uncompleted games start over with fresh clocks, as with existing resume behavior.
Completed games retain their original records and time control. Ratings spanning
this transition consequently combine time controls.

**Deployment requires a new coordinator as well as a clock-capable game bundle.**
A game-binary-only rolling update cannot give an older coordinator clock support.
For a rolling run, select the clock-capable bundle before resuming the new
coordinator with these flags. Preparation checks both executables; rolling
updates and rollbacks reject incompatible game binaries. Ordinary speed updates
after this transition still take effect between games.

The existing stop command aborts in-flight games; it does not drain them. Do not
mistake this one-time coordinator transition for a no-interruption rolling update.

## Records and verification

`state.json.time_control` and coordinator status expose initial time and increment.
Each game's `stats.clock` includes the control, an entry per move containing
`elapsed_ms` and `remaining_ms` (after increment), and optional `flag_fell`.
Old records remain readable. Partial aborted records retain clock telemetry.
No GUI startup or changes to model weights are needed.

Tests cover deterministic iteration admission, low-clock affordability, increments
and flag boundaries, real search cancellation/deadline fallback, per-side clock
accounting, record round trips, legacy state/resume, and incompatible binaries.

```sh
cargo test --test fischer_clock
cargo test training::clock
cargo test fischer_transition
python3 -m unittest discover -s deploy -p 'test_*.py'
```

This changes time-limited move selection. When merging, follow AGENTS.md's logic
snapshot rule against the actual merge parent before deployment.
