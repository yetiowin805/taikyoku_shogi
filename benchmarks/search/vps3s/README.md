# Three-second VPS follow-up

This follow-up freezes 24 positions before variant measurements: the original 18-position corpus plus six finite evaluation reversals. Both checkpoint families and C2/S2/L/A combinations are represented. This is a pragmatic expanded screen, not the original balanced six-group acceptance matrix; several positions share games, and only one prefix exceeds 1,000 plies.

The tournament and continuous analyzer are paused for this screen. Four independent benchmark lanes run concurrently, each pinned to one VPS CPU. Every case/repetition compares variants on the same CPU in shuffled order; CPU assignment rotates across repetitions. This measures performance under the intended four-search machine load, rather than an otherwise idle single-core environment. There are no concurrent builds or tournament searches.

- Three paired repetitions at 3,000 ms on every position, with depth cap 64.
- Baselines: unchanged main `9d3058696779ed1664c54e336d351c1ef006acdf`, and mechanical changes with storage disabled (`507`).
- Individual variants: swap-removal, restricted and stage-promoting TT-first, aspiration 100/500/2000, cross-turn hints, deadline intervals 8/32/128.
- Incremental mobility, interior PVS and retained TT bounds are excluded.
- One fixed-depth-2 main/mechanical comparison on each position; incomplete runs are not equal-work comparisons.
- Separate warm hint comparisons first search two recorded plies earlier using the same checkpoint and three-second budget, then search the target in the same process. Full original game prefixes are replayed; these are historical continuations, not games played by the variant.
- External per-process limit: 15 seconds, including parsing and the warm search. Screen deadline: 20 minutes, with a 21-minute process-group watchdog before resuming the original frozen tournament/analyzer configuration.

All search-changing options are behind `search-experiments` and disabled by default. Reusable storage is also disabled by default after the pilot's reproduced memory regression. The analyzer batching draft is not deployed by this benchmark.

## Reproduce

Build `examples/search_bench.rs` in release mode with `--features search-experiments`, and copy the executable to `BIN_DIR/experiments`. Build the same harness against main revision `9d30586`, without experimental features, as `BIN_DIR/stock`. The main harness overlay is instrumentation only. Release settings are thin LTO and one codegen unit. The binaries used here were built locally with Rust 1.92 and copied to the compatible x86-64 VPS to avoid rebuild downtime.

```sh
python3 benchmarks/search/run_vps_screen.py \
  --corpus "$PWD/benchmarks/search/vps3s/corpus.json" \
  --bins /absolute/path/to/BIN_DIR \
  --out /absolute/path/to/results --cpus 0,1,2,3 --seconds 1200
python3 benchmarks/search/summarize_vps_screen.py /absolute/path/to/results
```

The runner does not stop or restart services. VPS maintenance control was a separate bounded wrapper that restored the existing saved launcher configuration after the benchmark process group exited. Binaries, checkpoint identities and hardware are recorded in provenance. Raw rows include full move identity, ordered root lines, nodes, completed depth, scores, peak RSS, overshoot and experimental counters.

Timed node throughput is not a strength metric. Report changed moves alongside baseline's own repeat variability; comparisons at unequal completed depths are not score-equivalence checks. This screen does not provide independent tactical ground truth or tournament strength results.

## Data availability

Full game records, checkpoint copies and detailed raw results are deliberately excluded from Git. They remain in the originating local benchmark directories and, for the follow-up, `/opt/search-screen-20260913/` on the VPS. The checked-in corpus paths and SHA-256 identities identify the required inputs; copy the preserved inputs into the corresponding `inputs/` directory before reproducing.
