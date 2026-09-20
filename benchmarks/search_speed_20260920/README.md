# Search speed experiments, 2026-09-20

This study tests implementation changes against pinned main `aee552ee96416f3b6d5471e8f2498a64742eec03`. It uses two existing handcrafted checkpoints (`BASE_C2S2_A40_Lmate`, `C2K50A1`) and trained NNUE widths 512 and 2048 (`v2`). Model files and source games remain local, with identities frozen in the output corpus manifest.

## Protocol

- Replay complete prefixes from twelve distinct games: opening, a 160-ply midgame, a 360-ply tactical position, a 2,000-ply history, and eight independent early/middle/late positions. Preserve repetition history and draw counters.
- Select fixed search depth per position/agent using **stock only**, taking the last completed depth from a one-second calibration, capped at depth 3. Timed searches are calibration only; measured comparisons have no search deadline.
- Tune on the first four positions. Keep the last eight positions out of candidate selection. Before examining tuning outcomes, the confirmation set was expanded from four to all eight existing NNUE sanity positions. Choose the simplest combination within 1% of the fastest tuning mean (both evaluator families must improve), then perform a fresh, fixed-size confirmation run on all twelve. Report held-out results separately.
- Pin all benchmark processes to the same CPU (default CPU 2), execute searches sequentially, and do not build during measurement. Load models and replay positions outside the measured interval. Warm every position/agent/variant before measurement. Restart and rewarm both processes halfway through final confirmation to check a second process layout. An identical-binary A/A control estimates timing noise.
- Randomize case order per repetition and balance baseline/candidate order within each case. Record elapsed wall time and process CPU time for each search. Include fresh-search setup, allocation, cleanup, and table clearing. Reusable storage is measured after warm-up; no scores/bounds carry across searches.
- Require identical complete best routes, scores, node counts, q-node counts, completed depths, static scores, and ordered root results at fixed depth. Also verify the chosen complete route is legal. Stop at any mismatch, abort, process failure, or timeout.
- Primary endpoint (the eight held-out positions): geometric mean of paired stock/candidate wall-time ratios, with equal weighting of agents within positions and positions overall. Ratios above 1 mean faster. Bootstrap 20,000 samples at the **position-cluster** level for a 95% interval; use an exact two-sided sign-flip test on position means. Repetitions are repeated timings, not independent positions. Primary alpha is 0.05 for one selected candidate. Per-agent/family results are secondary, without multiplicity-adjusted claims.
- These are selected workloads on one machine, not a random sample of all games. Timing improvements do not establish playing-strength improvements. More nodes/depth under a clock requires a separate evaluation.

## Candidates

`variants.py` applies explicit source transformations to an isolated archive. The regular source checkout and production binaries are not used as scratch build directories.

- `qcache`: cache the last-royal capture flag used by the quiescence sort comparator.
- `iter`: iterate directions and capture rays without allocating vectors, preserving their order and stopping rules. Includes exhaustive path parity over all 1,679,616 square pairs and all 256 direction masks.
- `pool0`, `pool256`: recycle alpha-beta move vectors, varying initial reservation (0 or 256 moves). Only `pool0` was measured; the capacity variation was measured with the local pool below.
- `localpool0`, `localpool256`: reuse vectors within the search context instead of thread-local storage, varying initial capacity.
- `qcache-late`: compute ordering metadata after filtering, count royals once per node, and skip the work for singleton lists.
- `tt-clear`: recycle table backing storage and clear every entry between searches.
- `tt-dirty`: recycle storage and clear only slots written during a search; no entries remain visible across searches.
- Join names with `+` for combinations.

## Run

All outputs default to ignored `data/derived/search-speed-20260920/`. `prepare.py` currently locates the existing local NNUE study inputs; adapt those source paths when reproducing on another machine.

```sh
python3 benchmarks/search_speed_20260920/prepare.py
python3 benchmarks/search_speed_20260920/build.py stock qcache iter pool0 tt-clear tt-dirty
cp data/derived/search-speed-20260920/bin/stock data/derived/search-speed-20260920/bin/placebo
python3 benchmarks/search_speed_20260920/run.py pilot
python3 benchmarks/search_speed_20260920/run.py screen --variants placebo qcache iter pool0 tt-clear tt-dirty
python3 benchmarks/search_speed_20260920/analyze.py screen
python3 benchmarks/search_speed_20260920/build.py iter+qcache-late iter+qcache-late+tt-dirty iter+qcache-late+tt-dirty+localpool0 iter+qcache-late+tt-dirty+localpool256
python3 benchmarks/search_speed_20260920/run.py screen --label refinement --variants iter+qcache-late iter+qcache-late+tt-dirty iter+qcache-late+tt-dirty+localpool0 iter+qcache-late+tt-dirty+localpool256 --repeats 2
python3 benchmarks/search_speed_20260920/analyze.py refinement
# This was the selected candidate in the recorded study:
python3 benchmarks/search_speed_20260920/run.py confirm --variants iter+qcache-late+tt-dirty --repeats 16 --restart-every 8
python3 benchmarks/search_speed_20260920/analyze.py confirm
python3 -m unittest discover -s benchmarks/search_speed_20260920 -p 'test_*.py'
```

Build manifests record base revision, compiler, source hashes and binary SHA-256. Plans are written before each measurement phase. JSONL rows preserve each paired observation and its search fingerprint. Search diagnostics go to separate stderr logs.

## Read the recorded results

The committed `results/README.md` reports the completed study. `results/*.jsonl.gz` preserve the raw observations. To independently recompute any group without rerunning search:

```sh
python3 - <<'PY'
import gzip, json, sys
sys.path.insert(0, 'benchmarks/search_speed_20260920')
from analyze import summarize
variant = 'iter+qcache-late+tt-dirty'
with gzip.open(f'benchmarks/search_speed_20260920/results/confirm-{variant}.jsonl.gz', 'rt') as f:
    rows = [json.loads(line) for line in f]
print(json.dumps(summarize(rows, variant, positions=range(4, 12)), indent=2))
PY
```

`pool256` and adaptive table-clear transforms are supported exploratory implementations, but were not measured in this study. The recorded trial summaries are the authoritative list of tested candidates. Reusable table storage is a warm-process optimization; cold-start, densely filled tables at greater depth, and concurrent tournament workers were not benchmarked here.
