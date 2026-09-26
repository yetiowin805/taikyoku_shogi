# GPU training setup, experiments, and optimization

Status: implementation plan, 2026-09-25. The CPU bookkeeping improvements in
`pilot_fit.py` are implemented; CUDA training, custom kernels, larger effective
batches, and mixed precision are **not** enabled by this change. No GPU was
rented and the VPS was not changed. Return here before the next training run.

## Intended outcome

Train the existing ability NNUEs faster and more cheaply while the CPU server
continues games and search-based data collection. Preserve the feature schema,
fixed material baseline, probability objective, sampler, held-out groups,
quantization, and exported engine format initially. Measure software changes
separately from hardware changes. Do not use this work to replace tournament
models automatically or treat validation improvements as playing-strength proof.

The first deliverable is a reliable single-GPU trainer plus an end-to-end
benchmark. Architecture and data-quality experiments remain separate topics.

## Current workload and why a direct CUDA port is only a baseline

- `generation.py` launches `pilot_fit.py`, not the original `train.py` loop.
  Effective batch size is eight; the launcher sets microbatch size to one.
  Embeddings use sparse SGD without momentum or weight decay; the head uses Adam.
- Features are already packed into `features.bin` with per-position offsets.
  Samples and metadata come from `samples.jsonl`. No database migration is needed.
- The frozen pilot dataset has 58,112 training, 7,334 validation and 5,943 test
  positions. Training positions average 6,116.7 active feature entries across
  both perspectives; median 6,242, p95 8,990, maximum 9,648. These are unweighted
  dataset statistics; the weighted training sampler can give another average.
- There are 238,464 feature rows. Width 3072's FP32 embedding table alone is
  2.73 GiB. At the mean feature count, materializing one gradient row per feature
  occurrence costs about 573 MiB for batch eight, or 8.96 GiB for batch 128,
  before coalescing and other tensors. These are arithmetic estimates, not GPU
  peak-memory measurements.
- A CPU diagnostic with the first eight training records and width 32 observed
  65,980 gradient rows before coalescing and 5,234 afterward. This specific batch
  demonstrates repeated storage; it is not a representative duplication ratio.
- Quantization-aware forward computes both floating and rounded feature sums.
  The former supplies the straight-through gradient, the latter the forward
  value. Replacing this requires correct backward and rounding behavior.
- The two small hidden layers have only 32 outputs. GPU matrix-multiplication
  rankings and advertised tensor throughput do not predict our sparse workload.

## 1. Finish software preparation before paying for a GPU

Start a new branch from up-to-date main. Keep the original CPU path available as
the reference. Implement the following in a separate PR, then run CPU fixtures
before starting the paid session:

1. Add an explicit device selector with CPU as the compatible default. Requesting
   CUDA must fail loudly if it is unavailable; never silently benchmark CPU.
2. Place model, features, offsets, targets, material, outcomes and optimizer state
   on compatible devices. Construct optimizers after assigning/loading/moving
   the parameters. Keep FP32 master weights, accumulations and objective initially.
3. Preserve effective batch eight. Support microbatches 1, 2, 4 and 8; changing
   physical packing must not change which eight samples form an optimizer update.
4. Make validation and export device-aware. Export through bounded CPU chunks
   using the existing rounding and bias folding. Do not call `.numpy()` directly
   on a CUDA tensor or create several full-size CPU copies of the table.
5. Record device/runtime, driver, GPU, source hashes and optimization settings
   with every experiment. Pin helper files as well as the top-level trainer.
6. Add stage timers, peak memory and a bounded benchmark command. Keep detailed
   profiling separate from ordinary timing. Existing `benchmark_training.py` is
   CPU-only and is not the future CUDA benchmark command.

Do not bypass current recipe checks to continue an old output directory. The
trainer embeds its source hash and deliberately rejects code/recipe changes.
Use a new output directory. Add an explicit fork-from-training-state operation
if continuing an old optimizer state; verify the dataset, material baseline,
architecture and policy identities and record its parent. Starting from an
inference model is a warm start with new optimizers, not exact continuation.

### Recovery before long or interruptible runs

Existing checkpoints are saved after a completed pass. Add checkpoints every
5–10 minutes, only between complete optimizer updates, before using interruptible
rentals. Save model and both optimizer states, current pass and sample cursor,
deterministic sampled order or sufficient RNG state, CPU/CUDA RNG states, partial
training totals, learning rates, plateau counters and best checkpoint identity.
Persist a temporary file then atomically replace the committed state. Resume
must neither skip/repeat committed updates nor count validation twice.

Test forced termination before and after publication, partial temporary files,
and resume on the supported device. CPU-to-GPU continuation should preserve the
recipe but need not reproduce CPU floating-point arithmetic bit for bit. Report
it as a distinct execution lineage. Use progress-based supervision with enough
time for validation/export; distinguish a slow stage from a hung process.

## 2. Choose and configure the first rental

Start with **one on-demand RTX 3090 or A5000 with 24 GB VRAM**, preferably at
least 32 GB host RAM and 50–100 GB local disk. An A5000 listing with 25 GB host RAM
can also be tested after checking peak host memory. Prefer a reasonably modern
CPU and PCIe 3 x16 or better; avoid severely restricted PCIe links. Training is
not latency-sensitive to the VPS, so a cheaper region can be suitable.

The research snapshot found Vast 3090 offers around $0.19–$0.21/hour plus
bandwidth, Runpod A5000 at $0.27/hour, and Vast 4090/5090 offers near $0.49/hour.
These are historical quotes, not reservations. Check final CPU/RAM/disk/transfer
prices, host reliability, rental end date and CUDA support again when returning.
An RTX 4090 or 5090 is the second comparison; a 48 GB A40 is a capacity fallback
if the simpler path cannot fit, not an assumed speed upgrade.

Initial spending ceiling: **$5–10 total including setup and storage**, from the
existing proposal of $40 CPU / up to $50 GPU / $10 overhead per month. Use one
rental at a time. Set a wall-clock cap for the session and a provider spend cap
where available. Finish the first correctness/performance pass before spending
the rest. GPU hours include dependency installation and idle time.

1. Set up the provider account and SSH key when the user returns. Use an isolated
   project directory and a container/virtual environment. No provider is
   provisioned by this document or by the current PR.
2. Select a CUDA-capable PyTorch build matching the repository's pinned version
   and the host driver. Check the official installation matrix; do not copy the
   README's CPU-only wheel installation command. If the pinned version has no
   compatible wheel for a host, choose another host or separately validate a
   dependency change. Record the image digest and installed package versions.
3. Confirm the exact allocated GPU, memory, driver and host resources:

   ```sh
   nvidia-smi
   lscpu
   free -h
   df -h
   python -m pip freeze
   python - <<'PY'
   import torch
   assert torch.cuda.is_available(), 'CUDA unavailable; do not start the benchmark'
   print('torch', torch.__version__, 'runtime', torch.version.cuda)
   print(torch.cuda.get_device_name(0), torch.cuda.get_device_properties(0))
   x = torch.arange(4096, device='cuda', dtype=torch.float32)
   assert x.sum().item() == 8386560
   PY
   ```

4. Copy the pinned source snapshot, frozen dataset, parent descriptors and all
   referenced model blobs. Dataset files are `features.bin`, `samples.jsonl`,
   `schema.json`, `dataset.json`; preserve paths/identities used as split keys.
   Include frozen source games only for the later replay/search checks. Resolve
   any absolute descriptor paths in a staged copy without modifying originals.
5. Verify all manifest hashes, feature count/schema, sample splits and material
   identity on the rented machine before loading the training job. Keep the
   source and durable outputs on the VPS/local machine; the rental is disposable.
6. Keep inputs immutable and write each variant under its own experiment directory.
   Do not use a directory referenced by the live tournament as training output.

Official references: [PyTorch installation](https://pytorch.org/get-started/locally/),
[Runpod prices](https://www.runpod.io/pricing),
[Vast live offers](https://cloud.vast.ai/).

## 3. Establish correctness before measuring speed

Use identical initial weights and frozen input IDs for comparisons. Test:

- Packed feature order, perspective boundaries, duplicates, empty bags, and
  positions with few and many active features.
- Forward values, probability loss, sparse gradient indices/values, bias/head
  gradients, one optimizer update and several successive updates. Preserve zero
  gradient rows and merge duplicate contributions before clipping updated weights.
- Rounding ties, positive/negative weights at quantization limits, activation
  clamp boundaries and very small updates accumulated in FP32 master weights.
- All objective modes supported by the existing entrypoints; missing outcomes,
  both colors, fixed material addition, and unchanged train/validation grouping.
- Exported CPU/NumPy/Rust evaluation and legal complete search routes on the
  existing held-out search fixtures. A CUDA forward must not silently diverge
  from the evaluator actually used by games.
- Resume and export after interruption; no broken descriptor can be published.

CPU bookkeeping changes should give exact fixture parity. CPU/GPU reductions
may differ numerically: record absolute/relative errors, inspect rounding-threshold
cases, and compare against the existing CPU forward/export discrepancy. Do not
relax a tolerance simply to make an unexplained mismatch pass. Existing export
gates (mean below 50, max below 250 score units) remain outer limits, not targets
or a substitute for investigating additional GPU error.

## 4. A bounded first experiment

Freeze position IDs before looking at results. Include dense early positions,
median-density positions, sparse late positions and finite-score extremes from
the training/validation sets. Leave test positions for the final selected recipe.
Use widths **512 and 3072**; 1536 is an optional intermediate case if behavior
changes sharply. Avoid an exhaustive sweep of every width and GPU initially.

Compare on the first GPU, changing one thing at a time:

1. CPU reference, recorded separately with host details.
2. Straightforward CUDA FP32 port, effective batch 8 / microbatch 1.
3. CUDA FP32 with microbatch 8 and the same effective updates.
4. The third variant with cached metadata, prefetch and fewer scalar transfers.

Use the same deterministic weighted sample order and initial weights. For each
case, warm up at least 10 updates, then time 200 updates or enough to obtain a
20-second measurement, with a 10-minute cap per case. Record incomplete runs as
incomplete. Take three paired repetitions, alternate baseline/candidate order,
run one job at a time, and include peak host RAM and allocated/reserved VRAM.
If a full batch cannot fit, retry smaller physical microbatches and record that
configuration; do not silently change effective batch or skip large positions.

Measure normal timing and a short profiler trace separately. Use CUDA events or
explicit synchronization at timing boundaries so queued kernels are not mistaken
for finished work. Break down packing/transfers, feature forward, head/loss,
backward/coalescing, optimizer/clamping, validation, export and checkpoint writing.
Also time one complete pass with validation/export to expose costs hidden by the
update-only measurement. Count transfer and startup time in the rental invoice.

Only compare the second GPU after a working path is established. At observed
rates $0.49 versus $0.206/hour, the dearer instance needs approximately 2.38x the
throughput for lower compute cost per equivalent pass. Include overhead and
training quality in the final decision; raw examples/second is insufficient.

Store raw results, sampled IDs, dataset/model hashes, source revision, package
lock/image digest, driver, device, CPU allocation, seeds, loss scale and optimizer
settings. Record throughput, end-to-end pass time, dollar cost, memory and the
numerical comparisons. Do not run profilers or concurrent builds during timing.

## 5. Optimize the measured bottleneck

### A. Low-maintenance improvements first

- Reuse the current coalesced touched-row indices; benchmark prepacked immutable
  metadata on the GPU. A CPU metadata-cache prototype was held out because the
  timing evidence was mixed and validation regressions were not resolved.
  Keep the input as a flat list plus offsets; never build a dense
  batch-by-238464 input matrix.
- Prefetch packed batches into reusable pinned host buffers, transfer
  asynchronously, and establish correct buffer/stream ownership before reuse.
  Benchmark this against ordinary transfers; pinning each tiny temporary in the
  hot loop is not automatically a win. Try GPU-resident input data only after
  measuring the memory tradeoff, including index dtype expansion.
- Accumulate metrics on the device and retrieve summaries periodically. Retain
  nonfinite detection and failure reporting. Validate in larger physical batches
  independently of the training batch size; do not alter the validation sample
  set, weighting, metric definitions or checkpoint selection frequency.
- Reuse unique-index metadata within an update. Avoid a large per-position cache
  whose host-memory cost exceeds the sorting work saved.

### B. Quantization-aware feature transformer

The current duplicate floating/rounded forward is a high-priority profiler
candidate. First test a rounded-only validation path. For training, implement
a custom autograd operation with the same intended straight-through gradient,
then test every rounding/clamp boundary. Quantized rows may be cached while
weights are frozen during validation, or updated only for touched rows after SGD;
account explicitly for cache memory and invalidation after load/resume/update.

If temporary gradient storage/coalescing dominates, compare stock sparse and
dense backward with the same SGD settings before writing a custom kernel. A dense
gradient scans the full table, so its extra memory traffic may lose at small
batches. A custom CUDA/Triton operation can instead accumulate contributions by
feature row without materializing one full-width row per occurrence. Preserve
duplicate multiplicity, gradients through bias/clamps, and update/clipping order.
Avoid applying SGD separately for each occurrence, which changes rounding and
clipping semantics. Keep a tested PyTorch reference implementation.

Stockfish's [fused transformer code](https://github.com/official-stockfish/nnue-pytorch/blob/master/model/modules/feature_transformer/fused_ft_kernel.py)
is precedent for specialized forward/backward kernels, not a drop-in solution.
Our thousands of active features, shared ability rows, perspective mapping and
quantization need different memory/tiling choices and tests. Start with one GPU;
distributed training adds communication and recovery complexity without an
established need at this budget.

### C. Separate training-recipe experiments

After the mechanical path works, compare effective batches **8, 32 and 128**.
Use the same data exposure and report optimizer-update counts. A larger batch
has fewer updates per pass; unchanged learning rates are a baseline, not proof
that the new batch size is fairly tuned. Try a small explicitly recorded learning-
rate sweep for a promising batch, keeping validation cadence tied to equal data
exposure. Judge validation objective versus both wall time and spend. Retain the
best validation checkpoint and report plateau versus time/pass limits separately.

Mixed precision, changed optimizers and broad `torch.compile` are later experiments.
Start with FP32 embedding master weights and accumulation. Test lower precision
in the head or stored rounded rows separately; verify small updates and export
parity. The head is small, so do not assume tensor-core acceleration dominates.
Dynamic sparse shapes, sorting and Python synchronization need attention before
CUDA graphs or compilation can be assessed fairly. Compilation warmup and extra
memory count toward the experiment's cost.

## 6. Acceptance, rollout and cleanup

- Include a mechanical change when numerical checks pass and paired timings
  show a repeatable saving, or when a simple neutral change removes redundancy
  without material regression. Investigate >3% total slowdown, >10% in one input
  group or >10% peak-memory growth; repeat independently before accepting it.
- Run longer quality comparisons only for the best one or two configurations.
  Use validation for selection, then the held-out test once for the chosen recipe.
  Keep original seeds/data identities. A useful model needs existing admission
  checks and subsequent paired playing tests, not merely a faster training loop.
- Sync checkpoint, best model descriptor/blob, metrics and provenance to durable
  storage, verify hashes, and test restoration before ending the rental. Stopping
  a rental can leave billable disks; delete temporary instances/volumes when their
  contents are safely copied. Runpod network storage and volume disks have
  different idle prices; Vast storage/bandwidth are host-priced.
- Keep the present VPS and tournament manifest unchanged during this experiment.
  After GPU training is usable, the proposed CPU split is three games and one
  analysis worker. If GPU training drains its queue, stop paying for idle GPU
  time; consider additional CPU-generated labels instead.

Further references: [PyTorch tuning](https://docs.pytorch.org/tutorials/recipes/recipes/tuning_guide),
[profiler](https://docs.pytorch.org/tutorials/recipes/recipes/profiler_recipe.html),
[pinning/transfers](https://docs.pytorch.org/tutorials/intermediate/pinmem_nonblock.html),
[Runpod billing](https://docs.runpod.io/pods/pricing),
[Vast billing](https://docs.vast.ai/guides/instances/pricing).

## CPU changes available now

Both CPU trainers pack feature indices directly into their destination dtype,
avoiding an intermediate array and copy. Training computes only its requested
objective. Touched rows reuse the already-coalesced
gradient index, retaining the original fallback for callers accumulating prior
gradients. Validation converts each Huber vector once while keeping the original
Python accumulation order. Quantized forward, optimizer policy, microbatch size,
sample order and checkpoint format remain unchanged.

The default CPU entrypoints remain compatible. Code identity changes mean old
outputs continue using their pinned trainer, or are explicitly forked later;
do not edit their recipe hashes to force resume. No runtime restart is needed to
merge these changes. No historical engine snapshot is needed: this PR changes
training bookkeeping, not the engine's evaluation/search behavior.

Reproduce the paired CPU check locally, outside the correctness suite:

```sh
data/nnue-venv/bin/python -m unittest discover -s training/nnue -p 'test_*.py'
data/nnue-venv/bin/python training/nnue/benchmark_training.py \
  data/nnue-pilot-20260924/dataset \
  --baseline-rev 6b3397c0409a48bd1fa2c82deb4888b4a26c39f6 \
  --widths 512 1536 --pairs 3 --steps 8 \
  --out data/nnue-training-efficiency/paired-new.json
```

The benchmark alternates isolated baseline/candidate processes on one pinned CPU,
uses the same initialization and sampled positions, and requires identical losses,
validation metrics and final weight hashes. It reports update time, validation
time and process peak RSS separately, including per-update input preparation.
These short runs
exclude whole-pass checkpoint/export timing; they are not GPU speedup estimates.
