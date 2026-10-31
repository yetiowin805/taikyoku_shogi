# Short tournament CPU profiles

`profile_tourney.py` records one live game process with Linux `perf`. It defaults
to 30 seconds of userspace-only `cpu-clock` sampling at 19 Hz, without call
stacks or inherited child processes. It does not rebuild engines, alter PGO,
publish a binary, restart a game, or change kernel security settings.

Install a distribution-supported `perf` executable first. Ubuntu 26.04 packages
it as `linux-perf`; its kernel-tools package alone does not supply `perf`.
The process running the collector needs permission to profile the selected PID.
Use existing privileged operator access when required, rather than weakening
`perf_event_paranoid` globally. The collector makes no installation or permission
changes and disables debuginfod network lookups.

```sh
python3 deploy/profile_tourney.py --run-dir "$RUN" --out /private/NEW-profile \
  --pid auto --seconds 30 --frequency 19
```

Choose an explicit `--pid PID` to sample a particular game/build. `auto` chooses
the validated game with the most accumulated process CPU, which can be an older
engine still finishing a game after publication. One collector may operate per
run at a time. Outputs must be new directories; files are private (0600) within
a private directory (0700). Keep raw recordings on the host: DWARF mode copies
userspace stack memory, which can include application data.

The collector checks the exact command, request/result paths, running slot,
model pair, executable inode/hash and PID start time. It snapshots the request,
checkpoint hashes, immutable bundle, affinity and host boot ID. Both agents run
in one process: a profile identifies a **game and model pair**, not the side
currently thinking or an exact game phase. Historical subprocesses are excluded.
Checkpoint hashes describe the files observed on disk, under the tournament's
frozen-model policy; the collector cannot prove what a process loaded earlier
if an operator replaced a checkpoint in place.
Request disappearance, a changed process/model identity, zero/wrong-PID samples,
or failed/timed-out perf commands fail loudly and retain evidence. A game ending
naturally during capture can therefore leave a failed, partially useful profile.
Cleanup signals only collector-owned processes; it never signals the game.

`metadata.json` retains commands, timings, CPU accounting, sample count and
artifact hashes. `report.stdout` shows self sample shares by symbol; read its
lost-sample count and unresolved addresses before interpreting small differences.
Its "Overhead" column is sampled event share, **not observer slowdown**. Optimized
inlined code can be attributed to its enclosing symbol, and stripped/shared code
may remain unresolved. The exact immutable executable must remain available.
Recording and later report-processing CPU are recorded separately; these costs
do not measure all sampling work charged to the game/kernel.

For a short stack-quality check on the existing release binary:

```sh
python3 deploy/profile_tourney.py --run-dir "$RUN" --out /private/NEW-stacks \
  --pid PID --seconds 30 --frequency 19 --call-graph dwarf
```

This explicitly selects an 8 KiB stack dump rather than assuming the binary has
frame pointers. Inspect call-chain completeness with `perf report -i perf.data`
or `perf script -i perf.data`. Missing debug lines, truncated stacks and inlining
limit interpretation. Do not rebuild production solely to make bare `-g` work.

## Measure observer cost

Use an already-built `nnue_tournament_bench` and audited corpus from the PGO
workflow. The comparison reuses a warmed process per model, alternates attached
and detached identical search batches, and checks complete search signatures and
node counts. A perf control-pipe acknowledgement establishes readiness before
each attached batch. Registration and recorder CPU are retained separately.

```sh
python3 deploy/measure_profile_overhead.py \
  --binary /absolute/pgo/bin/benchmark --corpus /absolute/verify-corpus.json \
  --out /private/NEW-overhead --cpu ID --models 0,3 --positions 0,2 \
  --depth 2 --repeats 8 --batch-repeats 15 --frequency 19
```

Add `--call-graph dwarf` for a separate stack-sampling comparison. Arrange idle
compute where possible. On a fully occupied host, bound this auxiliary replay
with a low-priority systemd unit and CPU/memory limits, and preserve those limits
with the evidence. CPU quota and contention make wall-time comparisons noisy;
the runner reports search CPU separately. This benchmark does not acquire a game
CPU lease and should not become an unbounded background workload.

`pairs.jsonl` and `responses.jsonl` retain individual results. The summary uses
equal-pair geometric means and a deterministic paired bootstrap interval.
Repeated batches share roots/models and host load; the interval is conditional
on that workload, not a general guarantee. An upper interval above 1.01 does not
establish overhead below 1%. A noisy apparent improvement is not a profiler
speedup. Account for recorder CPU in addition to the engine's response timing.

Start with occasional short captures across different model pairs/builds, then
repeat after material engine changes. Establish cost and useful symbol/stack
coverage before enabling continuous collection. This tooling installs no daemon
or recurring timer and does not automatically retrain PGO. Compiler PGO still
requires an instrumented training build, profile merge, rebuild and validation.

## References

- [Upstream perf recording options](https://raw.githubusercontent.com/torvalds/linux/master/tools/perf/Documentation/perf-record.txt)
- [Linux perf permissions](https://www.kernel.org/doc/html/latest/admin-guide/perf-security.html)
- [Rust PGO workflow](https://doc.rust-lang.org/rustc/profile-guided-optimization.html)
