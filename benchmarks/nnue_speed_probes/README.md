# Local NNUE speed probes

These are opt-in measurement prototypes, not a production change. Enable the
`nnue-speed-probes` Cargo feature and choose `NNUE_SPEED_PROBE` = `baseline`,
`fused`, `snapshot`, or `both`. A normal build contains none of these prototypes.

See [measured results](RESULTS.md) for the selected next-run baseline and
[follow-up research](FOLLOWUP.md) for further ideas from established NNUE engines
and diagnostics of our current checkpoints. Follow-up ideas are not measured
whole-search gains over the combined baseline.

The [follow-up results](FURTHER_RESULTS.md) measure packed arithmetic,
royal-probe bypass, exact activation conversion, residual caching, and smaller
dense-head ablations. See [the follow-up protocol](FOLLOWUP_EXPERIMENTS.md) for
the quality checks, runtime flags, and reproduction commands.

- **fused:** a 4,096-slot direct-mapped cache sums all ability vectors for a
  piece/square/perspective into one i32 vector. Keys include schema piece identity,
  promotion/base-piece distinctions, color, square and perspective. Cache
  ownership is per accumulator and its model; cloning starts with an empty cache.
  Each populated slot costs roughly `4 * width` bytes plus metadata. Collisions
  replace the entry and never alter results.
- **snapshot:** save accumulator sums before making a move, and restore them on
  undo rather than reversing all feature deltas. Buffers are recycled in a pool
  owned by the accumulator. Changes on make still use the original update code.
- **native:** compile the same source with `RUSTFLAGS='-C target-cpu=native'`.
  This is a machine-specific binary and must be rebuilt for the deployment CPU.
  It also changes compiler optimization opportunities outside NNUE.
- **both/native-both:** combine the first two, with generic/native compilation.

No network, feature schema, checkpoint, search policy or live tournament changes.
The prototypes are intended to preserve integer accumulator values, scores,
chosen routes, move order and fixed-depth node counts. Timing-limited searches
can choose differently when they complete more work.

## Short ablation after the search-overhead merge

`post_merge_ablation.py` compares a native default-feature build with the full
cache/snapshot/packed/royal bundle, then removes one component at a time. Both
builds include main's merged ray iteration, quiescence ordering metadata, and
reusable empty TT storage. Each process performs an untimed depth-1 warmup before
its measured search so TT allocation reuse is exercised. The six configurations
share four frozen positions and widths 512/2048. Two balanced, shuffled
three-second repetitions produce 96 timed searches; 48 depth-2 searches verify
scores, complete routes, ordered root lines, and main/quiescence node counts.

```sh
ROOT=/home/frank/taikyoku_shogi
OUT="$ROOT/data/nnue-post-merge-ablation-20260920"
mkdir -p "$OUT/bin"
RUSTFLAGS='-C target-cpu=native' cargo build --offline --release --example nnue_speed_probe
cp target/release/examples/nnue_speed_probe "$OUT/bin/main"
RUSTFLAGS='-C target-cpu=native' cargo build --offline --release --features nnue-speed-probes --example nnue_speed_probe
cp target/release/examples/nnue_speed_probe "$OUT/bin/probes"
NNUE_SPEED_PROBE=both NNUE_FOLLOWUP=packed,royal RUSTFLAGS='-C target-cpu=native' \
  cargo test --offline --release --features nnue-speed-probes --lib nnue:: -- --test-threads=1
for phase in verify timed; do
  python3 benchmarks/nnue_speed_probes/post_merge_ablation.py --root "$ROOT" \
    --out "$OUT" --stock "$OUT/bin/main" --probes "$OUT/bin/probes" --phase "$phase" || break
done
python3 benchmarks/nnue_speed_probes/post_merge_summary.py "$OUT"
```

Use a new output directory on repetition; the runner refuses to overwrite raw
results. The main control compiles without the experimental feature, rather than
merely switching its flags off. Native compilation is held constant and is not
itself ablated. Baseline equivalence concerns fixed-depth results; timed searches
may finish different iterations. This small study does not measure Elo.

## Reproduce

Run from this branch's worktree. `ROOT` below is the existing data checkout,
containing admitted v2 models/blobs and the four frozen games recorded in
`data/nnue-speed-probe-20260920/manifest.json`.

```sh
ROOT=/home/frank/taikyoku_shogi
OUT="$ROOT/data/nnue-speed-experiments-20260920"
mkdir -p "$OUT/bin"
cargo build --offline --release --features nnue-speed-probes --example nnue_speed_probe
cp target/release/examples/nnue_speed_probe "$OUT/bin/generic"
for mode in baseline fused snapshot both; do
  NNUE_SPEED_PROBE="$mode" cargo test --offline --release --features nnue-speed-probes --lib nnue:: -- --test-threads=1
done
RUSTFLAGS='-C target-cpu=native' cargo build --offline --release --features nnue-speed-probes --example nnue_speed_probe
cp target/release/examples/nnue_speed_probe "$OUT/bin/native"
for phase in verify micro timed; do
  python3 benchmarks/nnue_speed_probes/run.py --root "$ROOT" --out "$OUT" \
    --generic "$OUT/bin/generic" --native "$OUT/bin/native" --phase "$phase"
done
python3 benchmarks/nnue_speed_probes/summarize.py "$OUT"
```

Finish all builds and tests before measurements. The runner launches searches
sequentially on CPU 0, with shuffled variants inside each model/position block.
The verification stage compares depth-1 full moves, ordered root lines, scores,
static scores, nodes and quiescence nodes. Every verification search must finish.
The timed stage uses two repetitions at 3 seconds with depth ceiling 8, for
widths 512 and 2048 at plies 0, 160, 360 and 2000. This is a small initial probe,
not a tournament or a comprehensive tactical regression suite.

The micro stage measures the output head, repeated remove/add of the first 64
pieces, and make/unmake of the first 32 legal moves. Five samples are retained.
These repeatedly reuse the same keys and are deliberately cache-friendly;
their gains are not predictions of full-search gains. Loading, initial accumulator
construction and replay are outside both search and micro timers. The search
itself clones the root and therefore starts with an empty experimental cache.
Peak RSS comes from `/usr/bin/time` over the full process, including loading.

Manifests retain game hashes, checkpoint descriptors/blob hashes, source hashes,
binary hashes, compiler/hardware details and the exact randomized order.
Raw JSONL includes search time, depths, scores, complete routes, root lines,
node/qnode counts and peak RSS. Results and large immutable models stay outside git.

The follow-up depth-2 check uses `--phase verify --verify-depth 2 --positions
opening,middlegame` and a separate output directory. A handcrafted compilation
control uses `--widths 0 --variants baseline,native --phase timed`; width 0 selects
`BASE_C2S2_A0_L0` and does not load NNUE. Use a separate directory for each follow-up
so the primary raw files are preserved.
