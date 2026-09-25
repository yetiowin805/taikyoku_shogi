# Eight-width generation v3

This run extends the frozen 512 pilot to widths 256, 384, 512, 768, 1024, 1536,
2048, and 3072. It uses the same immutable 71,389-position corpus and grouped
validation/test splits, fixed material, quantization-aware forward pass, and
score-probability targets without the game-outcome mixture. The five existing
widths warm-start from their frozen v2 models; the other three initialize fresh.

Each width draws 32,768 weighted samples per pass and uses effective batch 8,
accumulated one sample at a time with coalesced sparse gradients. Gradients are
checked against the full effective batch. Maximum training is 12 passes, with
validation-based plateau stopping and one learning-rate reduction. The best
validation checkpoint is retained, including the initial parent when no training
checkpoint improves it. No held-out test metric selects the checkpoint.

Scoring runs in a separate process after the trainer exits. This avoids retaining
two full floating input layers. A real 3072 training/export smoke test peaked at
about 3.6 GiB RSS, and a native search completed depth 2 with a legal move at 3s.
Completed optimizer checkpoints are removed after validation to bound disk usage;
inference models, metrics, recipe hashes and checks remain. Interrupted widths
retain optimizer state for resume.

`generation.py run CONFIG` must run under systemd pinned to CPU 3, with one-thread
math libraries, `MemoryMax=5G`, `RuntimeMaxSec=48h`, `KillMode=control-group`,
`TimeoutStopSec=360`, and `ExecStopPost=... generation.py restore CONFIG`.
It pauses the current analyzer only outside a search and shared-CPU lease, then
waits up to two hours for the fourth game to finish. Three game workers continue.
Timeout or failure resumes the original analyzer. Individual trainers have a
12-hour safety timeout. The whole job is expected to take roughly 12–24 hours,
not promised to finish before the hard limit.

All eight exports must pass model/content checks, improve on material validation,
and pass the frozen native-evaluator/search checks. Warm starts must improve their
validation objective. Any failure holds out the entire field and preserves the
current tournament. Test outcomes do not serve as an Elo or strength guarantee.

Automatic admission fits Bradley–Terry ratings to all completed games in the
current run, equally weighted, with a weak zero-centred regularizer of 0.1.
The lowest eight are retired at admission time. The 24 retained agents and eight
new immutable model IDs form a new 32-agent knockout run, retaining the original
512-v2 label teacher and adaptive 3/1 analysis sharing. The old run's files and
completed results are preserved; in-flight games are aborted by the existing stop
mechanism and can be requeued if that original run is resumed later.

New-run startup uses a separate persistent systemd unit so it outlives training.
Historical engine/helper pairs are copied with exact hashes and metadata. All
models are validated before stopping the old run. Startup failure rolls back to
the original run; ExecStopPost also recovers interrupted switchover. If the live
run, engine, teacher eligibility, or retirement selection changes incompatibly,
automatic admission fails rather than overwriting newer operator changes.

VPS paths for this run:
- Isolated code/config: `/opt/nnue-generation3-20260925/`
- Durable status, models, checks, ranking and rollout: `results/`
- Service: `nnue-generation3-20260925`
- Next run: `/opt/taikyoku_shogi/data/raw/tourney/royal-nnue-v3-32-20260925`

Inspect with `systemctl status nnue-generation3-20260925` and
`cat /opt/nnue-generation3-20260925/results/status.json`. Completed admission is
recorded in `results/rollout.json`; recovery is recorded in `restoration.json`
and `lease-state.json`. The live run pointer changes only after startup succeeds.
