# Low-CPU search experiments, 2026-09-26

Starting revision: `61c2875` (includes the small mechanical changes from PR #117
and the NNUE session changes from PR #118). These are **experimental source
transformations**, not enabled production changes. The provided driver restores
its six modified source files to the pinned baseline when it exits. Run it only
in a disposable, dedicated worktree.

See [REPORT.md](REPORT.md) for completed findings and explicit untested items.
The saved result patches, rather than all current prototype transformations,
identify the exact measured code.

## Resource and measurement policy

The user requested about an hour of investigation while on a video call. Every
build and benchmark runs sequentially, pinned to CPU 11 at niceness 19, with a
25% duty cycle on that one logical CPU. The supervisor resumes stopped processes
before terminating their process group on timeout/interruption. This avoids
stopped workers being stranded on those exit paths.

Compare **process CPU time**, not wall time: deliberate stopping and the user's
concurrent workload make ordinary wall-time speedup claims inappropriate. Even
CPU timings can vary with clock frequency, cache interference and scheduling.
These are screening results, not a tournament or a playing-strength estimate.

The first six positions (IDs 0,2,3,6,7,11) are the screen: opening, tactical,
2,000-ply history, two late positions and a late endgame. The other six
(1,4,5,8,9,10) are reserved before looking at variant results. All use the four
previously frozen agents (two handcrafted, NNUE512 and NNUE2048) and their
baseline-selected fixed depths. Models and complete game prefixes come from the
existing `search_speed_20260920` corpus. No live VPS process or checkpoint changes.

## Candidates

- **Blocker bitsets (A3/D3):** replace immutable capture-blocker hash sets with
  five 64-bit words. Keep exact piece membership. Direct NNUE blocker features sort names, but nested two-step features use Debug
  formatting. The original integrated test failed this compatibility check; the
  current formatting fix is untested.
- **Cached movement properties (A4/D1/D5):** precompute top-level two-step,
  capturing-range, only-capturing-range and range-direction fields at movement
  config construction. Preserve special promoted Whale/Rain Dragon configs and
  the distinct direction-adjustment rules of existing consumers.
- **Lazy leaf gates (A2/A7):** handle royal evasions first; return immediately
  for zero effective q-depth; otherwise try cheap sufficient capture-entry
  conditions before expensive discovery. Generate loud promotions for the leaf
  decision only when no capture-entry condition already succeeds.
- **Lazy labels (A7):** snapshot the piece and from/to/promotion information for
  progress labels, formatting only when a message is emitted. Preserve label
  text and deadline-check frequency; trace labels remain unchanged.
- **Line occupancy (A1/D3), component only:** compare scalar open-segment scans
  with masked 36-bit line words. This does not implement board updates, reverse
  attacker discovery or exotic/capturing-range rules.

## Reproduction

Use a disposable worktree based on `61c2875`; copy this benchmark directory and
the existing `benchmarks/search_speed_20260920/harness.rs` into it as
`examples/speed_experiment.rs`. Build the baseline before applying variants.
All heavy commands should be wrapped by `limit.py`; the driver does this itself.
`--target` may point at an existing Cargo target directory to reuse dependencies,
but no other build may use it concurrently.

```sh
python3 benchmarks/search_light_20260926/prepare_micro.py /tmp/light-micro
python3 benchmarks/search_light_20260926/limit.py --stats /tmp/micro-build.json -- \
  rustc --edition 2021 -C opt-level=3 /tmp/light-micro/main.rs -o /tmp/light-micro/bench
python3 benchmarks/search_light_20260926/limit.py --stats /tmp/micro-run.json -- \
  /tmp/light-micro/bench
# Full-engine build, then copy target/release/examples/speed_experiment to OUT/baseline:
python3 benchmarks/search_light_20260926/limit.py --stats /tmp/base-build.json -- \
  cargo build --offline --locked --release --example speed_experiment -j 1 --target-dir /tmp/light-target
# The driver expects OUT/baseline and an existing frozen corpus/cases file.
python3 benchmarks/search_light_20260926/driver.py lazy-gates bitset props \
  --out /tmp/light-results --cases /tmp/light-screen-cases.json \
  --corpus data/derived/search-speed-20260920/corpus.json \
  --target /tmp/light-target --max-wall 2200
```

The microbench uses actual pinned movement configuration/type/direction sources;
its minimal `Piece` shim provides only the three fields read by config lookup.
It checks all 303 piece types against 228 range sets from 305 canonical config
cases (including the two special promoted configs), and checks line occupancy
for every endpoint pair on the synthetic lines. Workload mixes are synthetic,
not measured game frequencies. Timings exclude one-time table construction.

Full-search runs reuse the existing exact-signature harness: complete chosen
routes, scores, ordinary/q nodes, completed depth, static scores and ordered root
lines must match; chosen routes must be legal. Each case is warmed, then measured
in shuffled, balanced pairs. The 120-second per-request watchdog remains active.

`rules_probe.rs` is a small full-engine fixture for reviewing proposed repetition
and TT shortcuts. It should be compiled as an example or linked against the
experiment's library; it is not part of a timing benchmark.
