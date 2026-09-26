# Small allocation and path-scan improvements

This follow-up implements four bounded parts of `ENGINE_SPEED_ANALYSIS.md`:
linear Simple generation/reach (D3), direct Jumping reach (D3), allocation-free
irreversibility queries (D4), and stack-based two-leg intermediates (D5.5).
No pruning, time management, evaluation, route ordering or rule changes are
intended. In particular, the Simple generator keeps its legacy duplicate enemy
landing when the following step exists. Irreversibility keeps the old handling
of jump orientation, including differences from actual jump generation.

## Correctness

- Exhaustively compare both intermediate sequences for all 1,679,616 square
  pairs, including candidate order, off-board intersections and duplicates.
- Compare reverse-reach decisions with the original capability union across all
  piece types, both colors, promoted configurations (including the special Whale
  and Rain Dragon cases), and every target from a central square.
- Compare raw Simple output with the original path-scanning implementation over
  every direction mask, both colors, four occupancy patterns, corners/interior,
  five distance limits and capture-only/full generation.
- Compare direct Simple and Jumping reach with generated landing membership,
  including asymmetric jumps, board edges, friendly/enemy blockers and zero or
  long jump offsets.

The benchmark uses the existing JSONL harness from
`../search_speed_20260920/harness.rs`. It rejects differences in full chosen
routes, scores, main/q nodes, depth, static evaluation and ordered root lines,
and checks that the chosen complete move is legal. Model loading and complete
prefix replay happen outside timing; search setup remains inside timing.

## Reproduction

Use the earlier corpus and its baseline-selected fixed depths (12 positions,
four agents: two handcrafted, NNUE512 and NNUE2048). It includes a 2,000-ply
history, early/middle/late positions and full original checkpoints. Those data
and model files are local, not duplicated in this PR. The original corpus
preparation instructions are in `../search_speed_20260920/README.md`.

Build the **same** existing harness on the document-only baseline `c34366b` and
this PR's code. Use a separate baseline worktree; do not overwrite working files.
For example, from this PR's checkout:

```sh
git worktree add --detach /tmp/engine-speed-baseline c34366b
cp benchmarks/search_speed_20260920/harness.rs /tmp/engine-speed-baseline/examples/speed_experiment.rs
cargo build --locked --release --manifest-path /tmp/engine-speed-baseline/Cargo.toml --example speed_experiment
cp benchmarks/search_speed_20260920/harness.rs examples/speed_experiment.rs
cargo build --locked --release --example speed_experiment
cargo test --lib
cargo test --release --lib
# Finish all builds/tests before measuring. Choose an otherwise idle CPU.
python3 benchmarks/engine_speed_20260926/run.py \
  --baseline /tmp/engine-speed-baseline/target/release/examples/speed_experiment \
  --candidate target/release/examples/speed_experiment \
  --corpus data/derived/search-speed-20260920/corpus.json \
  --cases data/derived/search-speed-20260920/cases.json \
  --cpu 2 --repeats 2 --out data/derived/engine-speed-followup
```

The output directory must not exist. The runner warms every case, balances
baseline/candidate order between repetitions, shuffles cases deterministically,
pins both processes to one CPU and runs searches sequentially. It records source
and binary hashes, compiler version, corpus identities, full paired records and
per-position/model ratios. A ratio below one means less elapsed time.

This is a quick regression/benefit screen, not a new held-out experiment or a
playing-strength test. Timing noise and code layout may mask small gains; do not
apply the original analysis's estimated multipliers to these four changes.
