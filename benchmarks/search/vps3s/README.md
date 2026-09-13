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

## Completed results

**1,038 target observations, zero process failures, 14 minutes 23 seconds of benchmark wall time.** Counts: 48 fixed-depth searches, 864 cold three-second searches, and 126 warm target searches (each with an additional preceding three-second search). All 24 fixed-depth comparisons completed. The tournament and analyzer resumed automatically with the original configuration and model snapshots; all 1,409 completed tournament slots and at least 6,337 cached searches were preserved. No experimental binary was deployed.

Throughput below is the equal-position geometric mean of each position’s median paired NPS ratio. Intervals are 95% position-bootstrap intervals, descriptive rather than independent-game confidence claims. All cold comparisons have 72 pairs. “Deeper/shallower” counts individual paired searches, not distinct positions. All three-second searches used their budget; larger NPS can include extra work in an incomplete iteration.

### Main versus mechanical baseline

With storage disabled, the mechanical build achieved **+9.3% NPS** (interval +6.2% to +12.8%), completed deeper in 5/72 pairs and shallower in none, and changed no chosen moves. Five score differences accompanied the deeper iterations. Fixed depth 2 preserved exact scores, full chosen routes, ordered root lines and node counts on every position; equal-position elapsed saving was 4.8%, and total elapsed saving was 5.2%.

The positive aggregate is not blanket acceptance. In the single fixed-depth check, the 1,031-ply case was 26.3% slower (117.8 → 148.8 ms) and one ending was 15.5% slower (297.3 → 343.5 ms). Two positions had peak RSS increases of 14.2% and 17.0%. These exceed the original investigation thresholds and need independent repeat/attribution before merging production defaults. In the primary three-second measurements, the ending and long-history groups were about 2.4–2.5% slower, and the worst paired mechanical RSS increase was 1.0%. Do not dismiss the fixed-depth findings merely because the timed aggregate looks good.

### Individual experiments versus mechanical baseline

- **Swap-removal:** +3.2% NPS (interval +1.0% to +5.4%); deeper/shallower 2/1; changed moves 3/72; changed scores 4/72.

- **Aspiration 100:** +3.6% NPS (interval -0.2% to +8.7%); deeper/shallower 2/6; changed moves 6/72; changed scores 20/72.

- **Aspiration 500:** +3.1% NPS (interval -1.3% to +9.5%); deeper/shallower 2/3; changed moves 3/72; changed scores 11/72.

- **Aspiration 2,000:** +0.4% NPS (interval -0.9% to +1.8%); deeper/shallower 2/3; changed moves 3/72; changed scores 11/72.

- **Restricted TT-first:** -0.1% NPS (interval -0.7% to +0.5%); deeper/shallower 2/1; changed moves 0/72; changed scores 3/72.

- **Stage-promoting TT-first:** -0.7% NPS (interval -2.5% to +1.4%); deeper/shallower 2/4; changed moves 3/72; changed scores 9/72.

- **Cold cross-turn hints:** +0.0% NPS (interval -0.7% to +0.8%); deeper/shallower 2/0; changed moves 0/72; changed scores 2/72.

- **Deadline interval 8:** -0.5% NPS (interval -1.2% to +0.2%); deeper/shallower 2/1; changed moves 0/72; changed scores 3/72.

- **Deadline interval 32:** -0.3% NPS (interval -1.4% to +0.7%); deeper/shallower 1/1; changed moves 0/72; changed scores 2/72.

- **Deadline interval 128:** -0.4% NPS (interval -1.2% to +0.5%); deeper/shallower 1/0; changed moves 0/72; changed scores 1/72.


Warm cross-turn hints had **21,239 accepted hint hits**, but **−0.15% NPS** (interval −0.74% to +0.40%), identical completed depths, chosen moves and scores in all 63 pairs across 21 eligible positions. Twelve ordered root-line outputs differed. The implementation was exercised; this is no longer merely an absence-of-hits result. Counters cover the process, including the warm-up.

Restricted TT-first made 1,588 probes and 918 immediate cutoffs; stage-promoting TT-first made 1,689 probes and 922 cutoffs. Both remain flat in aggregate despite more opportunity than the initial pilot. Aspiration retries were 234/114/33 for widths 100/500/2,000. The largest search-budget overshoot among cold variants was 11.4 ms (stage-promoting TT-first); none approached the external watchdog. Allocation counts were not measured; peak RSS is a process high-water mark.

### Search behavior and representative changed positions

No variant changed its chosen move between its own three repetitions on any position. Thus the repeated move differences below are not explained by baseline move-choice variability in this run. A few depth/score results varied at the time boundary; the raw data retain each repetition. Coordinates below are zero-based file,rank, and scores are from the side-to-move perspective. They are not independent tactical truth.

- **Swap-removal, `slot0569-middle`, applied ply 126:** baseline chose `(16,6)→(19,9)` at depth 2/3 depending on repetition; swap consistently chose `(16,6)→(16,7)` at depth 3, score −690. The baseline depth-2 score was −724. Swap’s changed move is therefore a behavior change under the actual budget, not a guaranteed improvement in play.

- **Aspiration 100, `slot0073-finite-reversal`, applied ply 29:** both completed depth 3. Baseline chose `(20,32)→(29,23)`, score −866; the variant chose `(28,26)→(27,25)`, score −1,320, in all three repetitions. This requires tactical inspection before a strength claim.

- **Aspiration 500 and 2,000, `slot0164-finite-reversal`, applied ply 428:** both completed depth 3. Baseline chose `(32,2)→(29,5)`, score −3,511; variants chose `(21,3)→(20,4)`, score −3,526, in all three repetitions.

- **Aspiration 100 and stage-promoting TT-first, `slot0764-middle`, applied ply 154:** baseline completed depth 3, score +7,003, choosing `(32,9)→(33,10)`. Both variants stopped at depth 2 with different moves: aspiration `(32,9)→(34,11)` at +7,263; TT-first `(18,25)→(18,31)` at +7,245. This is a concrete reason not to equate extra NPS with useful progress.

### Decisions

- **Mechanical changes included in draft; acceptance pending:** aggregate gain and exact fixed-depth parity are encouraging, but repeat and attribute the fixed-depth time/RSS outliers before merge. The existing broad attribution matrix remains outstanding.

- **Reusable storage held out:** disabled by default; the earlier independently reproduced +26% RSS regression remains the reason. The follow-up uses mask 507 throughout.

- **Swap-removal: promising tournament candidate after tactical review.** Its +3.2% NPS interval excludes zero, with particularly useful throughput gains in endings (+10.8%) and the one long-history case (+12.4%). It changes move choice and must remain an experiment.

- **Aspiration 100/500: mixed evidence.** Gains are concentrated in reversals, intervals include zero, and neither improves net completed-depth counts. Width 100 loses depth more often; 500 is the less concerning of the two, but this screen weakens the case for immediate tournament testing. Inspect the changed reversals first.

- **Aspiration 2,000: not promising as a speedup in this screen.** Near-zero throughput gain with changed behavior.

- **Restricted TT-first: mixed/inconclusive; stage-promoting TT-first: not promising in this screen.** The former is neutral, while the latter adds move changes and more depth losses without throughput benefit.

- **Cross-turn hints and deadline throttling: not promising as current speedup implementations.** Warm hints were exercised extensively and still flat. Deadline variants were all within noise and had no practical time-control advantage here.

- **Incremental mobility, interior PVS and retained TT bounds: held out.** Earlier large regressions exclude the first two; safe bound-hit feasibility should precede further retained-bound timing.

No combination or tournament was launched. The next useful work is resolving the mechanical outliers and inspecting swap/aspiration changed positions, rather than broadening the benchmark again.

### Detailed artifacts

`aggregates.json` contains per-position ratios, board-group results, bootstrap intervals, outcome counts, counters and repeat variability. `vps-provenance.json`, `build.json`, `corpus.json` and `input-hashes.json` preserve machine, build and input identities. Full raw routes/root lines and per-pair scores remain at `/tmp/vps-screen-results/results/` locally and `/opt/search-screen-20260913/results/` on the VPS. The source/test additions after the measured build do not change search behavior.
