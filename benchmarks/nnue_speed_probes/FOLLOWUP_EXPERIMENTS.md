# Follow-up experiment protocol

This study extends the selected native + fused-row-cache + accumulator-snapshot
baseline. All additions remain behind `nnue-speed-probes`, selected at process
startup with `NNUE_FOLLOWUP`. No production checkpoint or tournament is changed.

## Exact additions

- `quant`: the algebraically equivalent clamped 32-bit activation conversion.
- `packed`: an AVX2 byte dot product for the first dense layer. Inputs are at most
  127, so the saturating pairwise instruction cannot overflow its i16 result,
  even with weight -128. Each full dot product also fits i32. This initial kernel
  uses the existing weight layout; it does not test all possible packing,
  multi-output tiling, VNNI-specific kernels or width specialization.
- `royal`: temporarily detach the neural accumulator around the rule-only royal
  evasion probe, and restore the same object after normal completion or a failed
  make. Board moves and undo, history, material and rule checks are unchanged.
- `memo`: retain each perspective's computed neural residual within an
  accumulator, invalidating it on a change and restoring it with undo snapshots.
  This is not a cache of full evaluations or transposition-table search bounds.
- `stats`: diagnostic counters for residual calls, residual hits, accumulator
  change calls and royal probes, respectively. Never enabled during timing runs.

`exact` in the runner combines quant, packed, royal and memo.
`recommended` combines only packed and royal for the independent confirmation. `control` enables
none of them; both still use `NNUE_SPEED_PROBE=both` and native compilation.
`legacy` invokes the preserved accepted binary, detecting framework overhead.

## Quality-changing ablations

`head24` and `head16` calculate only 24 or 16 of the original first dense layer's
32 neurons. Other neurons take fixed integer activations estimated on calibration
positions. All trained weights remain frozen. Greedy removal minimizes output
score distortion from the original network on calibration data. These are quick
post-training pruning experiments, not newly trained smaller networks.

The sidecar `NNUE_HEAD_PLAN` is bound to the original blob's SHA-256 and width;
its retained indices and replacement values are validated at load time. The
runner combines pruning with all exact additions. It leaves feature-table size
and the second dense layer unchanged. Speed is compared with `exact`, not the
slower pre-optimization implementation.

Calibration: 256 positions from separate training games. Validation: 512 positions
sampled across 124 validation games, without overlap with calibration games.
The original networks previously used this validation split for early stopping,
so it is held out from pruning calibration, not a new unseen network test set.
Labels are saved engine search scores, not game-theoretic truth. The reference
residual target subtracts fixed material and clips to +/-100,000, matching the
trainer. Report score distortion separately from changes in teacher-label error.
The paired bootstrap resamples games, retaining within-game correlations.

`quality_checks.py` compares NumPy and Rust outputs on two validation positions
per width and pruned head (20 checks). Fixed-depth search comparisons measure
chosen-move changes separately from elapsed time; none of these measurements
constitutes an Elo estimate or proof of equal tournament strength.

## Reproduction

The accepted native binary in `data/nnue-speed-experiments-20260920/bin/native`
comes from commit 600a034 (also present at documentation-only revision bc48b93).
Build it in a separate worktree if reproducing from scratch. Reuse the frozen
four-game manifest from the first study, the five admitted v2 model descriptors
and blobs, and `data/nnue-training/dataset-v1`.

Run builds sequentially to completion, then copy and hash the finished executable
before beginning any measurements. Do not overlap builds or other intentional
CPU workloads with search timings.

```sh
ROOT=/home/frank/taikyoku_shogi
OUT="$ROOT/data/nnue-followup-experiments-20260920"
NNUE_SPEED_PROBE=both NNUE_FOLLOWUP=quant,packed,royal,memo \
  RUSTFLAGS='-C target-cpu=native' cargo test --offline --release \
  --features nnue-speed-probes --lib nnue:: -- --test-threads=1
RUSTFLAGS='-C target-cpu=native' cargo build --offline --release \
  --features nnue-speed-probes --example nnue_speed_probe
cargo check --offline
mkdir -p "$OUT/bin"
cp target/release/examples/nnue_speed_probe "$OUT/bin/native"
sha256sum target/release/examples/nnue_speed_probe "$OUT/bin/native"
"$ROOT/data/nnue-venv/bin/python" benchmarks/nnue_speed_probes/prune_head.py \
  --root "$ROOT" --out "$OUT"
"$ROOT/data/nnue-venv/bin/python" benchmarks/nnue_speed_probes/quality_checks.py \
  --root "$ROOT" --out "$OUT" --binary "$OUT/bin/native"
```

Run `followup_run.py --root "$ROOT" --out "$OUT" --binary "$OUT/bin/native"`
with these options, sequentially, stopping on failure:

- `--phase verify`: all nine variants, two widths, four positions, depth 2,
  30-second internal and 60-second external limits. Exact variants must match
  scores, static scores, full best routes, ordered root lines and node counts.
- `--phase micro --variants legacy,control,quant,packed,exact,head24,head16`:
  isolated output-head and make/unmake costs. Memoization is disabled in this
  phase so it measures actual inference, not repeated cache hits.
- `--phase counters --variants control,royal,memo,exact`: instrumented depth 2.
- `--phase quality-search --variants exact,head24,head16 --depth 1`: eight
  additional preselected held-out game positions at each tested width.
- `--phase timed`: two shuffled paired repetitions, three seconds, depth ceiling
  eight, four saved positions, widths 512/2048, nine variants, one pinned CPU.

Output files are created exclusively: use a fresh output directory for a new
batch. Plans and quality-corpus files must be present there for pruning and
quality-search phases. The runner stores exact order, source diff, binary/model/
game hashes, compiler, hardware, timing, RSS, complete moves, root lines and nodes.
`followup_summary.py "$OUT"` produces the compact report data.

The optional `--fixture benchmarks/nnue_speed_probes/royal-fixture.json` selects a
small synthetic double-check position for targeted parity/counter tests. Keep
these measurements separate from the saved-game aggregates.

The initial follow-up search batch accidentally used a stale executable copied
before release linking completed. Its files are retained under
`invalid-old-binary/`, explicitly excluded from all final results. The corrected
runner requires follow-up telemetry from the new binary; Rust pruning parity and
binary hashes independently confirm that the intended implementation is running.

## Confirmation and targeted fixture

After the main batch, use a separate output directory and run `--phase verify
--variants legacy,control,recommended`, followed by `--phase timed --variants
legacy,control,recommended`. These need no pruning plans. The second batch uses
the same binary and frozen positions, with 24 verification and 48 timed runs.

The targeted fixture uses another separate output directory, `--fixture
benchmarks/nnue_speed_probes/royal-fixture.json --phase verify --depth 3
--variants legacy,control,royal,exact`. It adds eight exact comparisons.
