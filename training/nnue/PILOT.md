# 512 data and loss pilot

This is an isolated experiment, not a replacement for the production trainer or
the tournament's checkpoints. It compares four 512×2–32–32–1 networks:

- `fresh-huber`: fresh initialization, existing residual Huber loss.
- `warm-huber`: initialize from the deployed parent's quantized parameters,
  reset optimizers, use residual Huber loss.
- `warm-wdl`: same parent initialization, fit search-score expected scores.
- `warm-wdl10`: same, with 10% observed outcome in the expected-score target.

The last two separate changing the loss space from adding outcomes. The pilot
keeps material values, feature mapping, inference format and search settings
unchanged. Expected scores use a single symmetric sigmoid temperature fitted
on **training games only**, with equal game weight. This is a practical
calibration, not a claim that score alone captures all outcome probabilities.
Draws have no outcome component because saved records do not distinguish rules
draws from move-limit draws. They can still supply search-score labels.

All four arms use quantization-aware forward passes: active feature rows, biases,
dense weights and activations follow the deployed rounding rules. Floating master
weights and straight-through sparse gradients retain sub-quantum updates. Only
active rows are quantized per batch, avoiding a full embedding-table copy.
This was necessary after ordinary floating-point continuation failed the existing
export-error gate. The gate was retained, and the corrected recipe was restarted
consistently for every arm; preliminary rejected models are not compared as final
candidates. The production inference implementation is unchanged.

## Data

`pilot_snapshot.py` streams completed immutable game files and a consistent
read-only SQLite query of completed training-label records. It does not stop or
modify the live run. Keep the archive's hash and the parent checkpoint/blob.

`pilot_data.py` combines this snapshot with the old frozen corpus. It requests
up to 64 spaced representative positions per recent game and 24 per older game,
balanced by side, plus existing analyzed positions. Analyzed scores supersede
saved scores at the same position. Analyzer one-based move indices are converted
to the exporter's zero-based prefix lengths. Full replay and feature extraction
use the existing Rust exporter. Terminal and mate-range examples are excluded.

Pairs with the same tournament start seed and games with identical 64-move
prefixes remain in one split. Any group previously used to train the parent
remains training data. Old validation groups remain validation data. Otherwise
groups are hash-assigned 80/10/10 to train/validation/test. Canonical feature
duplicates are removed, including held-out positions previously seen in parent
training. This prevents known leakage, not every possible near-duplicate opening.

Training draws have effective weights 60% representative NNUE-labelled
positions, 20% targeted analysis positions, 20% handcrafted-labelled positions.
All arms use the same deterministic sequence of weighted draws, 32,768 per
pass. These are sampling passes, not exhaustive epochs over the corpus. Longer
search labels are teacher targets, not ground truth; 10 seconds does not always
complete an additional depth. No new labels or games enter after freezing.

## Training and comparison

Each arm has a maximum of eight sampling passes, a four-pass minimum before
plateau stopping, two-pass patience, a 0.5% improvement threshold and one learning
rate halving. A maximum-pass stop does **not** mean a plateau was reached. The
best validation checkpoint is exported; the final test split never selects an
epoch. Resumption checks the dataset, parent, code and recipe identities.

The selected export must agree with floating-point inference within the existing
quantization tolerances. `pilot_compare.py verify` additionally compares 16
independent NumPy integer evaluations against Rust and checks complete legal
search routes at three seconds. The match runner replays the entire prefix,
including repetitions and draw counters, for both color assignments.

Four held-out, approximately balanced late-game starts are fixed **before** any
candidate result is examined. Each candidate plays the frozen parent from both
colors, at 3 seconds per move and the tournament's depth-eight ceiling. The
160-ply screening cap is **unresolved**, never
a draw or an evaluation-based adjudication. These eight games per candidate
are a small continuation screen, not a full opening tournament or an Elo estimate.
Report results by pair, including unresolved games and watchdog failures.

## Reproduction

Run from the repository root. Raw assets and models live under gitignored `data/`.
The reference experiment uses `data/nnue-pilot-20260924`, the frozen old corpus
under `data/nnue-training`, and the deployed parent under `data/nnue-admission-v2`.

```sh
# On the VPS, stream to a local archive; the Python program itself arrives on stdin.
mkdir -p data/nnue-pilot-20260924
ssh root@62.238.5.45 'cd /opt/taikyoku_shogi && python3 - royal-nnue-32-20260920T030938Z royal-nnue-v2-32-20260920T152202Z' \
  < training/nnue/pilot_snapshot.py > data/nnue-pilot-20260924/snapshot.tar.gz

data/nnue-venv/bin/python training/nnue/pilot_data.py data/nnue-pilot-20260924
data/nnue-venv/bin/python training/nnue/pilot_compare.py freeze data/nnue-pilot-20260924

OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 taskset -c 0 \
  data/nnue-venv/bin/python training/nnue/pilot_fit.py data/nnue-pilot-20260924/dataset \
  --parent data/nnue-admission-v2/NNUE_W512_v2.json \
  --out data/nnue-pilot-20260924/warm-huber --init parent --loss huber --epochs 8
# Repeat with the other recipes above; warm-wdl10 adds --mix 0.1.

RUSTFLAGS='-C target-cpu=native' cargo build --release --bin nnue_pilot_match
data/nnue-venv/bin/python training/nnue/pilot_finish.py data/nnue-pilot-20260924
```

The finishing supervisor is specific to this local workstation: it uses its four
homogeneous efficiency cores 4, 6, 8 and 10, and waits until training has exited
before running checks and matches. Choose suitable CPUs on another machine.
`status.json`, `metrics.json`, `test.json`, `checks.json`, per-game JSONL logs,
and full continuation game records preserve the evidence. Completed matches
can be resumed only with matching model, binary and settings identities.

```sh
OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 data/nnue-venv/bin/python \
  -m unittest discover -s training/nnue -p 'test_*.py'
```

### Bounded VPS run (2026-09-24)

`pilot_vps.py` runs an isolated copy under systemd, pinned to CPU 3. It pauses the
existing analyzer only when it has no child search and holds no lock, reserves
`analysis.request`, and takes the existing shared lease after any fourth game
finishes. Three tournament workers remain untouched. `ExecStopPost` resumes the
same analyzer PID/start-time identity on success, failure, or service timeout;
normal adaptive analysis/game sharing then resumes. No model is deployed.

This screen uses six sampling passes per arm, a 65-minute per-arm training limit,
and a 10.5-hour service limit including CPU handoff. Saved best completed exports
from budget-limited training are eligible for comparison and marked incomplete
in `vps-status.json`. All four arms run sequentially, followed by sequential
3-second paired games. Capped games remain unresolved. The service must set
`KillMode=control-group`, `CPUAffinity=3`, one-thread math-library environment,
and `ExecStopPost=... pilot_vps.py restore .../vps-config.json`.

`vps-status.json`, `restoration.json`, `common-test.json`, each arm's
`comparison.json`, and final `comparison.json` are the durable records. Game
paths relocate without modifying dataset identities; selected game hashes are
checked before replay. Portable binaries use `target-cpu=x86-64-v3`.
