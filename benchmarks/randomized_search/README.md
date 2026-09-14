# Randomized three-second search combinations

This benchmark samples all eight `BASE_P120H50B75_C2…` agents equally. It freezes 100 positions per agent from 20 shuffled completed tournament games, balancing early/middle/late plies. Each pair searches the same complete game prefix and original model on the same pinned CPU, with randomized execution order and a 3,000 ms search budget.

The 24 configurations are the factorial combinations of:

- Swap-removal: off/on.
- Aspiration half-width: off/100/500/2,000.
- TT-first generation: off/restricted/stage-promoting.

`S0-W0-T0` is the mechanical baseline, with reusable storage disabled (mask 507). Half the planned comparisons use this baseline against a random nonbaseline configuration; half choose two distinct random configurations. Positions are sampled with replacement from the frozen corpus. Every block of eight pairs covers all eight agents; jobs are divided among four pinned lanes. Completed counts will differ slightly at the wall-clock boundary. Each configuration resets search state in a fresh process.

This PR does **not** change production search code. `engine.patch` contains the experimental search fork from draft PR #100, applied only inside a temporary archive of pinned main revision `9d30586`. The normal checkout, normal build targets, analyzer protocol, model weights and tournament defaults are untouched. This deliberately allows experiments to proceed without merging that draft's unresolved production regressions. The patch is an experiment snapshot; regenerate/review it when rebasing the experimental engine.

Incremental mobility, interior PVS, retained TT bounds, cross-turn hints and deadline throttling remain off in this study. The unused implementations in the patch are retained from its source snapshot, but the harness exposes only the three listed factors. Experimental configurations change search behavior; neither extra NPS nor nominal depth establishes playing strength.

## Build and run

```sh
python3 benchmarks/randomized_search/build_engine.py --out /absolute/path/bin
python3 benchmarks/randomized_search/prepare_corpus.py \
  --repo /opt/taikyoku_shogi \
  --run /opt/taikyoku_shogi/data/raw/tourney/RUN_ID \
  --out /absolute/path/corpus
python3 benchmarks/randomized_search/run.py \
  --binary /absolute/path/bin/search_bench \
  --corpus /absolute/path/corpus/corpus.json \
  --out /absolute/path/results --seconds 10800 --cpus 0,1,2,3
python3 benchmarks/randomized_search/summarize.py /absolute/path/results
```

The build uses Rust release mode, thin LTO, one codegen unit, locked dependencies and `search-experiments`. The output includes the base/tool revisions and hashes of the patch, harness and binary. The corpus manifest identifies each immutable game and model by SHA-256; inputs and raw results stay local/on the VPS, outside Git.

The maintenance wrapper preflights one position per agent and verifies corpus identities before stopping services. It then invokes the existing supervisor stop operation, backs up checkpointed state/catalogue/models/binaries, runs the bounded benchmark, and restores the **original frozen configuration** in `finally`. In-flight tournament games follow the existing stop/resume behavior: they restart from their beginnings; completed games are retained. The wrapper has an internal benchmark deadline and kills its process group before restoration. Launch it detached under an additional host watchdog, for example:

```sh
nohup timeout --kill-after=180s 11040s \
  python3 benchmarks/randomized_search/maintenance.py \
  --repo /opt/taikyoku_shogi \
  --run /opt/taikyoku_shogi/data/raw/tourney/RUN_ID \
  --binary /absolute/path/bin/search_bench \
  --corpus /absolute/path/corpus/corpus.json \
  --out /absolute/path/maintenance --seconds 10800 \
  > /absolute/path/maintenance.log 2>&1 < /dev/null &
```

Do not run another benchmark or build on the VPS during measurement. Restoration occurs before summary computation. `maintenance.json` reports restoration success/failure explicitly; `results/cpu*.status.json` report counts, agent balance and failures while running. `results/finished.json` and `summary.json` mark completed measurement and summarized results. Five search failures in one lane stop the study rather than silently spending hours on a broken configuration. A hard host failure/SIGKILL can still require operator restoration; inspect status after any unexpected termination.

## Interpretation

Raw observations include nodes, quiescence nodes, elapsed time, completed depth, full chosen route, ordered root lines, scores, peak RSS, timeout overshoot and experimental counters. The streaming summary reports direct comparisons with the mechanical baseline, equal-weighted by agent, and retains all random-vs-random pairs for interaction analysis. Do not treat repeated samples from the same position/game as independent evidence. Final uncertainty analysis should cluster by source game and report per-agent/group results, selected tactical changes, and incomplete/failed pairs separately. No tournament trial is automatically launched.

Validation: Python scheduler and timeout/failure tests are outside the default correctness suite; timing runs are always opt-in. The isolated fork carries the source draft's regression tests; production builds do not include that fork.
