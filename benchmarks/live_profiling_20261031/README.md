# Live tournament profiling pilot — October 31, 2026

The bounded `perf` collector is deployed on the tournament VPS and produces
useful flat function profiles of the existing release binaries. Routine DWARF
caller attribution is rejected for this release because its observed call
chains are incomplete. This pilot identifies optimization targets; it does not
measure a new engine speedup or playing-strength change.

Tooling revision: `945c3def3f42cd64539c4eb988fdd074b17052db`.
[Aggregate evidence](pilot.json) retains timestamps, source/build/tool hashes,
capture counts, function sample counts, observer-cost results and limitations.
Raw recordings, requests, complete reports and replay responses remain private
on the VPS under `taikyoku/profiling/pilot-20261031/`. The production engine,
models, coordinator and tournament configuration were not replaced by this pilot.

## Definitely good: bounded flat profiles

Environment: AMD EPYC-Genoa, eight vCPUs, Ubuntu 26.04, kernel
`7.0.0-34-generic`, perf 7.0.14. The eight tournament workers remained active.
Recordings target one PID at a time, userspace `cpu-clock:u`, 19 Hz, 30 seconds,
without child inheritance. Every capture validates the exact request, running
slot, model pair, executable hash and PID start time before and after sampling.

| Capture | Actual engine | Agent pair | Samples | Raw bytes |
| --- | --- | --- | ---: | ---: |
| `pgo-flat-1` | PGO `96b2cdb8…` | W512 v3 / W512 v3 HLFN | 568 | 34,696 |
| `old-flat-1` | Older `114cd924…` | W512 v3 N / W512 v3 HLFN V | 569 | 34,736 |
| `pgo-flat-g3-1` | PGO `96b2cdb8…` | W512 v3 HLF G3SC / W512 v3 HLFS G3GA | 568 | 34,696 |

There were **1,705 flat samples across three games/model pairs**, with zero
reported lost samples. Unresolved symbol samples were 29/568, 21/569 and 38/568
(3.69%–6.69%). Existing release symbols are sufficient for function-level work;
they do not provide source lines or reliably recover inlined callees.

In the first PGO capture, `is_position_attacked_by_color_filtered` had 18.31% of
self samples, NNUE `Accumulator::residual` 9.86%, `make_move_for_search` 9.33%,
and `generate_captures_hitting_square` 6.16%. In the G3 pair, capture generation
had 13.38%, attack checking 11.62%, residual evaluation 9.68% and move-state
updates 7.39%. These are individual symbol shares of sampled user CPU, not
inclusive subsystem totals. Different games/positions/policies prevent a
before/after performance comparison between captures.

**Next targets suggested by these samples:** attack queries, quiescence capture
generation/filtering, search make/unmake work and NNUE residual evaluation.
These are leads, not accepted speedups. Match fixed-work behavior and measure
changes before promoting any candidate.

The collector plus its reaped perf/report children used approximately
0.40–0.42 process-CPU seconds per flat capture, including setup and report
processing. That is direct collection cost, not a measurement of all slowdown:
sampling work charged to the game or kernel is outside this accounting.

## Promising: small observed search cost; the 1% budget remains unproven

The same PGO benchmark executable (`e2fbe160…`, full SHA in the evidence) served
both arms. Its source content identity is `5b859b15…`, compiled with Rust 1.97.1 /
LLVM 22.1.6 and host-native PGO. Compare perf detached versus attached at 19 Hz,
without call stacks. The workload used two models (`BASE_C2S2_A0_Ldefense`,
`NNUE_W512_v3`), two roots from different held-out games (plies 323 and zero),
depth two, four balanced repetitions and five identical searches per batch.
There were **16 paired batches / 160 measured searches**, plus four warmups.
Every search signature and node count matched. All sixteen attached batches
contained samples (94 total, four to ten per batch).

| Metric: geometric mean per five-search batch | Detached | Attached | Relative change | Conditional paired 95% interval for change |
| --- | ---: | ---: | ---: | ---: |
| Search process CPU (ms) | 310.692 | 311.050 | +0.115% | −0.904% to +1.152% |
| Search wall time under quota (ms) | 6,174.598 | 6,209.724 | +0.569% | −0.777% to +2.068% |

The statistic gives each paired batch equal weight; intervals use 3,000 seeded
paired bootstrap draws. Timing sums the five engine-reported search durations,
excluding load/replay and attachment. Repeated pairs share only two models,
two roots and host contention, so these are conditional intervals, not a general production
bound. The CPU interval extends above +1%: **this pilot does not establish the
proposed sub-1% search-cost budget**. Negative interval endpoints reflect noise,
not a profiling speedup. NNUE alone had +0.518% observed CPU cost, with a wider
−0.915% to +2.152% conditional interval.

The auxiliary experiment used CPU 0, niceness 10, a quota of 5% of one CPU,
1 GiB memory and a 900-second deadline, while all eight tournament workers
remained active. The quota ceiling is 0.625% of eight-core capacity for this
auxiliary cgroup; it is not a bound on sampling work charged elsewhere. Quota
throttling explains the large wall-time values and limits their relevance to
ordinary games. Registration is excluded from search timing and separately
recorded. Sixteen short-lived recorder attachments consumed 4.209 CPU seconds;
that setup-heavy total is not an estimate of steady live profiling cost.

An initial attempt stopped safely on perf's optional NUL-terminated ACK; the
parser now accepts its exact two supported forms, with regression coverage.
A longer second attempt was stopped only to shorten auxiliary replay time under
the quota; its partial pairs remain on the host and are excluded from the final
comparison. The completed workload and all raw pairs are retained separately.

**Decision:** use occasional short flat captures after meaningful engine changes
or when investigating a slow agent. They already identify useful work with
small files and low observed search cost. Establish a tighter overhead bound on
broader/deeper workloads before enabling always-on collection. The current PGO
build remains selected; this pilot does not rebuild it or refresh compiler data.

## Rejected: routine DWARF caller attribution on this release

The separate PGO DWARF capture used the same 19 Hz/30-second settings with an
8 KiB userspace stack dump. It recorded 568 samples, zero reported losses and
4,800,960 raw bytes, versus 34,696 bytes for the flat capture.

Of 568 events, 193 had no decoded chain and 475 had at most one known frame.
Only 93 had two or more known frames, which still does not establish valid
unwinding; several observed parent addresses were unresolved or implausible.
Therefore this evidence cannot support inclusive flamegraphs or caller-cost
claims. Keep the low-volume flat mode as the default. A future diagnostic build
with verified unwind/debug information needs its own correctness/performance
comparison before use in rated games.

## Operation and validation

The collector and observer runner are opt-in, with new private output
directories, bounded subprocess deadlines and cleanup limited to their own
children. No daemon or recurring timer was installed. Existing `perf_event_paranoid=4`
and `kptr_restrict=1` were preserved; existing privileged operator access was
used. Ubuntu's `linux-perf` package supplies the profiler on this host. Package
installation required no kernel or tournament restart.

Local checks passed: Rust 1.97.0 formatting, all-target/all-feature Clippy with
warnings denied, and 531 Rust tests (four ignored); Ruff 0.16.8 formatting/lint;
214 deployment Python tests, three randomized-search tests, 25 tactical-tree
tests and 49 NNUE Python tests. Sixteen new tests cover identity rejection,
private outputs, lock conflicts, PID reuse, sampler process cleanup, perf
failures, exact search parity, order balancing and sample/ACK validation.

Operational instructions and the matched-cost method are in
[deploy/PROFILING.md](../../deploy/PROFILING.md). Checkpoint hashes in the evidence
are observations of on-disk files under the frozen-model policy; they are not
proofs of earlier loaded-memory contents. One game process alternates agents,
so exact side/ply/phase attribution is unavailable. Historical child engines
are outside the recorded process scope.
