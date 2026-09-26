# Ability NNUE starter models

For the expanded training recipe, see [generation v3](GENERATION.md). Future
data collection and mate-position experiments are recorded in the
[NNUE data-quality roadmap](DATA_ROADMAP.md); these proposals are not enabled by
this document.

The [GPU setup and optimization plan](GPU_PLAN.md) records the rental setup,
correctness gates, bounded experiments, recovery requirements and CPU improvements
to complete before the next GPU training session. CUDA training is not yet enabled.

Five evaluators share the handcrafted engines' search (including S2, swap-removal
and aspiration 500), but replace positional evaluation with a trained residual
on **fixed material**. Accumulator widths: **512, 768, 1024, 1536, 2048** per
perspective; shared feature table; head `2W -> 32 -> 32 -> 1`.

The input uses the engine's movement abilities, occupancy and royal status. It
reserves 92 channels per color/square, of which 85 currently occur. No separate
piece identity or ray-blocking channels are added. Promotions and the special
Rain Dragon/Whale provenance rules use the actual movement configuration. White
jumps preserve the existing engine's rank-only reflection, including asymmetric
pieces; ordinary directions use its 180-degree rotation.

The model does not decide terminal wins, repetition or progress draws. Those
remain search/game rules. Scores retain the existing engine scale: network
output times 1000 plus the fixed material score, with nonterminal bounds below
the mate range. Material values also remain available to search ordering and
pruning. A/L handcrafted positional terms do not contribute to NNUE evaluation.

## Train locally

Python 3.12 with CPU PyTorch; a GPU is not required. Training is sequential across
widths, with four PyTorch threads by default. Sparse SGD avoids dense optimizer
state for the large input table; the small head uses Adam. No tournament process
is touched by these commands.

```sh
python3 -m venv data/nnue-venv
data/nnue-venv/bin/pip install -r training/nnue/requirements.txt --index-url https://download.pytorch.org/whl/cpu
cargo build --release --bin nnue_tool --bin analyze_position
# On the machine holding the tournament data, from the repository root:
python3 training/nnue/collect.py > /path/to/corpus.tar.gz
# Place the archive under data/nnue-training/corpus.tar.gz locally.
python3 training/nnue/prepare.py data/nnue-training
target/release/nnue_tool export data/nnue-training/base.json data/nnue-training/jobs.jsonl data/nnue-training/dataset
data/nnue-venv/bin/python training/nnue/train_all.py data/nnue-training/dataset --base data/nnue-training/base.json --out data/nnue-training/models-v1
```

The collector samples 300 completed games from each of four recent tournaments,
checks completion/abort status, and deduplicates full trajectories. The preparer
selects up to 12 finite-score positions per game, stratified over game progress.
A stable hash of the start plus first 64 moves assigns whole groups to validation;
positions are then deduplicated across the entire export. This limits correlated
position leakage and keeps long games from dominating. It does not eliminate
all opening-family correlation or teacher bias. Mate-range scores are excluded.
Saved search scores describe the position **before** the recorded move and are
black-absolute; export converts them to side-to-move perspective.

The Rust exporter replays each complete prefix once, including history and draw
state. Its feature schema and mappings are the same ones used for inference.
`features.bin`, `samples.jsonl`, corpus manifest, schema and hashed `dataset.json` preserve sample
identities and labels. All dataset components and the fixed material checkpoint are hash-checked before training or resume.

A single width can be trained with `train.py --width WIDTH` and the same paths.
`--resume` restores the last **completed epoch**, optimizers and deterministic
sample order. Interrupted epochs repeat; the best completed validation checkpoint
is retained. Network files are content-addressed; publishing a newer descriptor
does not overwrite the previous model's weights. Use a new output directory when changing the data, seed, baseline,
learning rates or architecture. Do not train into a directory used by a live run.

## Quantization and sanity gates

Input weights use int16 /4096; accumulators use int32 because this board has many
more active features than chess. Both small hidden layers use int8 /64 weights
and clipped 0–127 activations; the final 32-value dot product uses float32. The
shared first-layer bias and the dense biases are exported explicitly. Centering
the input to the first small layer during training is folded into its bias on
export, without adding inference work. This prevents the early hidden-layer
saturation observed with an uncentered, high-learning-rate pilot.

Models have a versioned binary header, exact byte-length checks, a feature-mapping
hash and a SHA-256 content identity. Loaded weights are immutable and shared by
Arc across players when applicable. Search states maintain per-position
accumulators across make/unmake, captures, promotions and clones. Switching
models rebinds the accumulator; mutating/reinitializing a board invalidates it.
A declared NNUE checkpoint that cannot load must never silently become a seed
engine. Handcrafted checkpoint behavior remains compatible.

Each best export is compared with floating-point inference on 32 validation
positions. `validate.py` additionally checks independent NumPy integer inference
against Rust, measures the entire held-out corpus, rejects constant corrections
or held-out loss worse than material alone, and requires legal completed searches
on eight spread-out positions per model using the tournament's three-second
budget. Legality compares the complete route with generated moves. Only then does it write `ready.json`.
These are entry sanity gates, **not** evidence of tournament strength.

```sh
cargo test --lib
cargo test --release --lib
python3 -m unittest discover -s deploy -p test_tourney_analysis.py
data/nnue-venv/bin/python -m unittest discover -s training/nnue -p test_training.py
```

## Replace the tournament field

Use `stage.py MODELS OUTPUT` to select the five admitted model JSONs, their
referenced binary weights, metrics and `ready.json`. Copy that staging directory to
an immutable VPS directory such as `models/nnue-v1/`. Training state files and raw
games are not deployment requirements and are not committed to GitHub.

```sh
python3 deploy/prepare_nnue_field.py --source OLD_RUN/state.json --models models/nnue-v1 --out models/nnue-v1/manifest.json
python3 deploy/tourney_analysis.py stop --run-dir OLD_RUN
python3 deploy/tourney_analysis.py start --run-dir NEW_RUN --manifest models/nnue-v1/manifest.json --carry-analysis-from OLD_RUN --cpus 0,1,2,3 --depth 8 --time-ms 3000
```

Prepare/validate before stopping the old run. Back up its state/config first.
The new manifest removes exactly the five `_Lold` agents and retains the other
27 entries. Use a **new run ID**: never assign NNUE games to retired agents'
identities or rewrite old ratings. Existing stop semantics abort in-flight games;
the old saved run remains resumable. The new run starts a fresh bracket field.

The existing supervisor still shares CPUs 3/1 between tournament and analyzer,
returning the fourth CPU when analysis is idle. Analyzer model snapshots now also
pin NNUE blobs and rewrite their local dependency paths. Carried catalogue entries
retain their original model/engine identities. Rollback is to stop the new run
and resume the preserved old run; do not delete its model or analyzer snapshots.

## Continue training beside the tournament

The coordinator also supports `--sidecar training`: three CPUs play games while
the fourth continues **512, 768, 1024, 1536, 2048**, sequentially, with one PyTorch
thread. Position analysis is paused; its saved catalogue remains available.
Training waits for any game already holding the fourth CPU to finish. When all
five widths finish, or training fails, that CPU returns to games automatically.
Analysis stays paused. A failure is visible in status and logs and requires an
operator restart; it is not silently retried forever.

The first continuation uses the same frozen dataset, validation split, fixed
material baseline, optimizer state and sample order as the two-epoch starter
models. It does not ingest newly played games. For each width, monitor held-out
Huber loss, require at least **8 total epochs**, and count **0.5% relative loss
reduction** as meaningful improvement. After three epochs without meaningful
improvement, halve both learning rates; allow two such reductions, then stop
after another three unproductive epochs. Small improvements can accumulate to
the threshold. **32 total epochs** is a safety cap, reported as `max_epochs`, not
as evidence that the model plateaued. These are practical first-pass stopping
settings, not a claim that validation loss measures playing strength.

Each completed epoch atomically saves the latest training state, optimizers,
plateau counters and learning rates. Interrupted epochs repeat on resume.
The exported model is the **best validation epoch**, which may precede the latest
training state. Starter files and current tournament weights stay immutable:
continuation writes a separate `v2` generation and never changes the live field.
Run the quantization/search admission checks above before considering those
models for a later tournament. Repeated tuning on this validation set will need
a separate final test set to assess generalization.

Prepare these inputs on the VPS **before stopping anything**:

- CPU PyTorch environment installed with `training/nnue/requirements.txt`.
- `dataset-v1/` containing `features.bin`, `samples.jsonl`, `schema.json`, and
  `dataset.json`, plus the original `base.json`. Raw games are unnecessary for
  continuation; game paths in the samples are split identities only.
- The five original `wWIDTH.training.pt` files from the admitted
  `models-final-v1/` training output, copied into `seed-state/`. These contain
  optimizer state and are additional to the deployed inference models.
- The deployed `models/nnue-v1/` descriptors and their referenced blobs.

The initial dataset is about 342 MiB and the five training states total about
5.3 GiB. Keep room for another full set of training states, best exports, and
temporary atomic replacements (roughly 13 GiB of additional output headroom).
Restore uses memory-mapped tensors without allocating a second full network.
Only one width is resident for training at a time. Old exports in this private
output directory are removed once no model descriptor references them; source
models are never pruned. Do not use the mutable training directory as a live
tournament model directory.

From `/opt/taikyoku_shogi`, after the PR is merged and pulled:

```bash
python3 -m venv data/nnue-venv
data/nnue-venv/bin/pip install -r training/nnue/requirements.txt --index-url https://download.pytorch.org/whl/cpu
# Copy the inputs described above before preparing the job.
data/nnue-venv/bin/python training/nnue/continuation.py prepare \
  --dataset data/nnue-training/dataset-v1 --base data/nnue-training/base.json \
  --checkpoints data/nnue-training/seed-state --seed-models models/nnue-v1 \
  --out data/nnue-continuation/v2 --python data/nnue-venv/bin/python \
  --config data/run/nnue-training-v2.json
run=$(cat data/run/royal-nnue-current-run.txt)
python3 deploy/tourney_analysis.py check-training --run-dir "$run" \
  --training-config data/run/nnue-training-v2.json
```

`check-training` is read-only: it verifies dependencies, input hashes, dimensions,
sample counts, material values and output isolation without touching live
processes. Launch repeats preflight before starting children and pins the
training program and configuration under `RUN/training/`. Resume uses that
pinned program. Changing data, code or policy requires a new output directory;
an existing output refuses a mismatched job identity. The prepared config may
be edited before its first launch.

After preflight passes, use the switch commands in
[the deployment guide](../../deploy/README.md#switch-the-fourth-cpu-from-analysis-to-nnue-training).
For progress, use the usual coordinator `status` command, or inspect
`RUN/training/trainer.log`, `OUTPUT/progress.json`, `OUTPUT/wWIDTH.status.json`,
`OUTPUT/wWIDTH.metrics.json`, and `OUTPUT/wWIDTH.log`. Status includes per-width
completion reasons, the current epoch/sample progress, and CPU allocation.

Tests use tiny networks and process fixtures, without touching the VPS:

```bash
data/nnue-venv/bin/python -m unittest discover -s training/nnue -p 'test_*.py'
python3 -m unittest discover -s deploy -p 'test_*sidecar.py'
```
