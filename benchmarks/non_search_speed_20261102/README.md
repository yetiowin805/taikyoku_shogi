# Non-search engine performance follow-up

Baseline: main `a28150ff16194682b6849491c673f0de0e7c67c6`. Measurements use portable release builds, ThinLTO, one codegen unit, Rust 1.92.0/LLVM 21.1.3 on an Intel Core i7-1255U, pinned to CPU 2. Compilation and test suites were paused for recorded comparisons. The host was not isolated from unrelated activity.

Six board snapshots come from three recently completed corrected-rules VPS games: slots 0703, 0700 and 0633, completed 2 November UTC. Source plies are 24, 701, 161, 1400, 420 and 931, with 706, 412, 660, 167, 482 and 254 pieces. Three roots have each color to move. Phase labels describe development through late game, not a formal material-phase classifier. Export provenance includes source hashes and the matching live replay engine hash. Full games and model weights were not copied from the VPS. Snapshot roots reset move/repetition history and use canonical snapshot ordering, so they are representative board workloads, not exact live search traces.

Search uses the current main implementation with the latest G3ATK, G3FP and G3PT policies. Locally available W512-v3 NNUE and non-NNUE weights match the VPS hashes. Descriptor names, paths and search settings were reconstructed locally; these are not byte-identical copies of remote checkpoints or runs of the custom live backport.

## Results and acceptance

The final set reduces legal-generation batch means by **3.97–43.76% across the six VPS roots**. The opening control improves 3.73%. All 91 ordered API count/checksum comparisons match. Ten samples per arm are repeated batch means from two counterbalanced executable passes; ranges are in [final-api-summary.json](final-api-summary.json), not individual-request percentiles.

Reused-output opening generation drops from 97 to 74 allocation/reallocation calls; the late 1,400-ply root drops from 261 to 53. All 56 allocation cases preserve move counts. Requested bytes are cumulative allocation traffic, not RSS: see [final-allocation-summary.json](final-allocation-summary.json).

Whole search is much less conclusive. In 180 final pairs (six roots × three policies × ten balanced repetitions), geometric-mean process CPU falls **94.7315 → 94.2019 ms**, a **0.56% point estimate**. The three-source-game cluster bootstrap ratio interval is **0.990475–1.000086**. The adjacent 72-pair same-binary control differs by **1.54%**, so the small search difference cannot be confidently attributed to these changes. The PR accepts clear reductions in allocation, copying, hashing and irrelevant ray traversal; it does **not** claim an established 0.5% whole-search gain. No playing-strength or realistic-clock claim.

There are 576 timed baseline/candidate pairs and 144 same-binary control pairs across screens and confirmations. Every full search signature matches (move, score, depth, nodes, q-nodes, static evaluation and root lines). An initial CLI/server warmup failure yielded no candidate measurement; only its valid same-binary control is retained as evidence. Search setup is timed; checkpoint loading, snapshot loading and legal-route validation are outside the search timer.

The final code excludes fixed heap arrays, exported special-piece dispatch, and small-list unstable sorting. Fixed arrays failed the search screen; dispatch slowed common reach queries and source review found a legacy metadata compatibility problem. Stable sort remains below 64 targets; zero/one-target unions skip dedup entirely. Long Free Eagle paths retain an overflow fallback and unchanged JSON, while generated paths fit inline. Public Rust path types change to `MovePath`/borrowed slices; no Copy promise is made.

## Evidence and limits

- [Search summaries](search-summaries.json): exact per-policy/per-position values, repetitions, node counts, CPU/wall timing, ranges and bootstrap intervals. [Raw search records](raw/search/) include plans, hashes, warmups and individual pairs. Three game clusters and persistent process placement do not quantify all host/systematic noise.
- [Movement experiments](speed-movement-findings.json), [representation experiments](speed-representation-findings.json), [state experiments](speed-state-findings.json): all 44 originally promising/speculative follow-up areas, including four API/lifecycle designs with no meaningful speed candidate. Seven previously reproduced bugs remain tracked separately in PR #209.
- [Raw kernels](raw/) include all six roots. Table/mailbox/occupancy, packing, configuration, maintained-count, binary-write and checkpoint experiments are bounded kernels. Their construction, mutation, decoding, cache competition or complete integration costs are explicitly excluded where unimplemented. They establish no search gain.
- The mixed geometry-table primary covers all 5,041 distinct deltas with equal call boundaries. Earlier diagonal-only and unequal-call-boundary rows are diagnostic and superseded, not general geometry evidence.
- [Candidate patches](patches/) preserve experimental identities. Trial patches include rejected components and must not be deployed wholesale. The final production diff selects the refined implementation.
- [Manifest](manifest.json) and [validation](validation.json) record revisions, binary/source/input hashes and actual checks. Full Ruff still reports 30 existing errors in unrelated untracked analysis/backup files; repository scope passes. No changes to the live VPS tournament.

## Reproduce

Run from repository root. This requires the matching existing local W512-v3 network, not network transfer:

```sh
python3 deploy/prepare_snapshot_bench.py benchmarks/non_search_speed_20261102/workload.json /tmp/core-bench-inputs
cargo build --release --example nnue_tournament_bench --example speed_movement_screen --example speed_movement_allocations --example state_speed_probe --example non_search_representation_probe --example speed_movement_spec
taskset -c 2 target/release/examples/speed_movement_screen benchmarks/non_search_speed_20261102/positions.jsonl
taskset -c 2 target/release/examples/speed_movement_allocations benchmarks/non_search_speed_20261102/positions.jsonl
```

Build baseline and candidate JSONL servers from their recorded source revisions/patches in separate target directories, without native/PGO flags, and preserve each executable before changing source. Do not reuse older checkout mtimes in one shared Cargo target cache. Then run `deploy/benchmark_search.py` with those two binaries, the prepared `corpus.json`, this directory's `cases.json`, `--cpu 2 --repeats 10 --aa-repeats 4`, a fresh output directory, and explicit baseline/candidate revision identifiers. Keep compilation/tests separate from recorded timing. Remaining probe input formats are documented at the top of each example; raw files and the findings record exact workloads and repetition counts.
