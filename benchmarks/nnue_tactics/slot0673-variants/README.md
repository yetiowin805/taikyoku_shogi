# Slot 673 search experiments

This is an off-default search-policy investigation, not a playing-strength test.
The feature-gated `tactical_probe` binary consumes the complete game prefix and
uses the exact frozen W512 v4.5 checkpoint for every trial. Production entrypoints
and the live tournament are unaffected.

Read [REPORT.md](REPORT.md) for findings. `summary.json` contains compact data
from 457 searches; `raw-results.tar.xz` preserves every raw output and manifest.
Extract the archive in this directory to restore `runs/` before regenerating
the summary. `verification.json` records final baseline checks and the archive
hash. `source-v1/README.md` explains reconstruction of the first measured build.

## Frozen positions

`corpus.json` was frozen before variant results. The primary cases are the boards
before White's 174th and 176th plies in slot 673. The ten controls come from ten
other games and cover early, middle, late, long-history, and drawn games. These
are cost and sensitivity controls: they do not have trusted optimal-move labels.

The ply is the one-based index in `moves`, **not** the record's `move_number`.
Hashes identify the complete original games, checkpoint JSON, and NNUE blob.
Private game/model files are referenced under `--data-root`, not copied into Git.

The two target labels are deliberately limited:

- At ply 174, the historical choice is `17,31-17,1+`, a Woodland Demon capture
  promoting to Old Peng. A move starting `18,13-` moves the threatened Tengu and
  may capture the attacking Fierce Ox. Such a choice is a candidate improvement,
  not automatically a proven save or optimal move.
- At ply 176, `17,1-18,0` immediately captures the Crown Prince. The historical
  choice is the Dragon King's pawn capture/promotion, `26,26-26,10+`. Other moves
  require line inspection; a later Crown Prince capture can be tactically valid.

## Reproduction

Build `tactical_probe` with `--features search-experiments`, using the compiler
and flags in the run's build record. Build first, then run the benchmark; do not benchmark concurrently
with a build. The runner uses `nice -n 10`, a single CPU chosen with `taskset`,
sequential processes, and a reproducible shuffled order.

```sh
python3 benchmarks/nnue_tactics/slot0673-variants/run.py run \
  --binary /absolute/path/to/tactical_probe \
  --data-root /home/frank/taikyoku_shogi \
  --group target --mode fixed --fixed-limit-ms 20000 --no-soft-stop \
  --output /absolute/path/to/results/singles

python3 benchmarks/nnue_tactics/slot0673-variants/run.py run \
  --binary /absolute/path/to/tactical_probe \
  --data-root /home/frank/taikyoku_shogi \
  --variants baseline,q_evasions,q_tt_context \
  --group all --mode timed --timed-ms 3000 --repetitions 3 \
  --output /absolute/path/to/results/timed-controls
```

The fixed-depth targets are depth 3 at ply 174, depth 4 at ply 176, and depth 3
for controls unless overridden. The 20-second search budget limits expensive
screening variants. `--no-soft-stop` prevents predictive iterative-deepening
stopping from ending a fixed-depth trial early while retaining its deadline.
The initial `singles` screen used the original predictive stop, so some
incomplete trials ended before 20 seconds; subsequent fixed-depth follow-ups
disable that prediction. Timed tests retain normal time management.
A separate process safety timeout preserves completed JSONL
iterations if a diagnostic process fails to respect its search deadline.
Incomplete depth 4 and completed depth 4 are never equal-depth comparisons.
Timed runs request depth 64 and compare the last completed iteration at the
same time allowance. Process wall time includes model loading and full replay;
the engine's elapsed time measures search separately.

Each output directory contains a manifest with the binary hash and complete
configuration, source revision and hashes, compiler identification, per-trial JSONL/stderr, and parsed JSON including root lines,
completed depth, score, nodes, final quiescence nodes, return status, and time.
Rerunning an identical command resumes missing trials. A different binary or
configuration requires a new output directory. Parent-shell `TACTICAL_*`
and `TAIKYOKU_AB_*` variables are removed so they cannot contaminate baseline runs.
Earlier phases stripped `TACTICAL_*`; their emitted configurations were audited
and contained only the requested q depths, so no inherited-q-depth contamination
was observed. The final probe also explicitly pins the checkpoint's q depth.

`--dump-pv` enables diagnostic TT-following PV traces; use separate runs for
timing. A TT-following trace is not a proof and can stop early or encounter an
incompatible bound. Root lines can likewise contain PVS bounds, so they must not
be interpreted as exact scores for ranking every candidate. Restrict the root
to a single encoded route via `TACTICAL_ROOT` for independent candidate searches.
Those restricted searches are diagnostic and are not directly equivalent to a
full-root tournament search.

## Interpretation

`variants.json` lists independent interventions. Later combinations must receive
new named entries or a separate variants file, preserving the initial run.
Tempo interpolation changes evaluation; forcing a capture instead of stand pat
can suppress valid quiet moves. Neither is a behavior-preserving optimization.
More favorable choices on the two development positions do not establish Elo
gain, general correctness, or a safe default. Controls are for detecting broad
cost and behavior changes, followed by independent tactical tests and games.

## Switch reference

These environment variables are accepted by the diagnostic probe with the
`search-experiments` feature. Boolean values are exactly `0` or `1`; unknown
names and invalid values fail. Production entrypoints do not read these search
controls. Defaults preserve existing behavior.

- `TACTICAL_Q_EVASIONS=1`: apply the existing last-royal evasion handling inside
  recursive qsearch, including at q-depth zero, before stand pat or qTT use.
- `TACTICAL_Q_TT=normal|hints|context`: normal cache behavior; ordering hints
  without score cutoffs; or scores keyed by the selective q context.
- `TACTICAL_FULL_NONPV_Q=1`: retain the full configured q budget at non-PV leaves.
- `TACTICAL_ALWAYS_Q=1`: allow capture q at leaves without the usual tactical
  entry gate. The configured depth and later candidate filters still apply.
- `TACTICAL_Q_ALL_CAPTURES=1`: generate all legal enemy-capture routes and eligible
  loud promotions, bypassing S1/S2 and PathAware eligibility restrictions. This
  **does not search every capture**: the default policy still applies a top-N cap
  (eight ordinary candidates outside PathAware) and remaining pruning. Combine
  deliberately with other switches to study those restrictions separately.
- `TACTICAL_Q_NO_DELTA=1`: disable whole-node and per-move material delta pruning.
  `TACTICAL_Q_DELTA_MARGIN=N` instead adds a nonnegative evaluation-unit margin.
- `TACTICAL_NO_HANG=1`: disable AB and q hanging-capture rejection; this does not
  independently widen the capture generator's eligibility rules.
- `TACTICAL_NO_NULL=1`: disable null-move pruning.
- `TACTICAL_LMR=normal|evasions|off|interior_off|root_off`: retain reductions,
  protect last-royal check evasions, disable both reduction sites, or disable
  only the named site. `evasions` does not exempt every quiet checking move or
  threatened-major escape.
- `TACTICAL_STAND_PAT=normal|major_capture_required`: the second choice removes
  stand pat as the guaranteed floor when a major enemy has just landed on the
  previous destination and a capture continuation is available. This is a
  deliberately nonstandard, potentially unsound diagnostic: it can force an
  undesirable capture instead of allowing a valid quiet move. A static fallback
  remains when no candidate is searched.
- `TACTICAL_TEMPO_PERCENT=0..100`: retain that fraction of the NNUE's same-board
  side-to-move component globally. `100` is unchanged; `0` averages the two
  Black-perspective scores. Material, terminal, and draw semantics are retained.
  This changes evaluation, not just search speed, and may suppress real initiative.
- `TACTICAL_Q_TEMPO_PERCENT=0..100`: below `100`, apply that percentage to the raw
  network at q stand-pat evaluation, including q-depth-zero evaluation. It
  overrides the global percentage there rather than multiplying it. Its default
  `100` uses ordinary evaluation, which inherits any global tempo setting.
  Quiet leaves bypassing q remain under the ordinary/global evaluator.
- `TACTICAL_QDEPTH=N`: override the q depth, retaining mandatory entry evasions.
  `TACTICAL_QBROAD=1` is the earlier selective broadening: baseline q pruning,
  any-capture entry, and no S2-only filter. It is not equivalent to generating
  every legal capture.
- `TACTICAL_ROOT=ROUTE[;ROUTE...]`: restrict the root to uniquely resolved, complete
  encoded routes, including intermediate squares and promotion. Each route must
  match exactly one legal move.
- `TACTICAL_DUMP_PV=1`: emit diagnostic `PVTRACE` JSON to stderr after completed
  iterations. The harness exposes this as `--dump-pv`; timing phases leave it off.
- `TACTICAL_NO_SOFT_STOP=1`: keep attempting the next requested depth until the
  unchanged hard deadline. The harness exposes this as `--no-soft-stop`.

## Experiment phases

The committed run manifests contain the exact selected variants, repetitions,
limits, CPU, binary hash, source hashes, and build declarations. Output directory
names distinguish these phases:

1. `feature-baseline`: verify original depth-3/depth-4 moves, scores, and exact
   node counts against production. `singles`: 24 single settings on both targets,
   one run each, 20-second ceiling and ordinary CLI predictive stopping.
2. `combinations`: ten named settings on both targets with a 20-second hard
   ceiling and predictive stopping disabled. `fixed-repeats`: three shuffled
   repetitions of baseline, global half-tempo, q-only half-tempo, contextual qTT,
   and forced major capture at the original target depths.
3. `deeper174`, `deeper176`, and `deeper-zero-*`: baseline, half-tempo, forced
   capture, and then zero-tempo at depth 4/5, with a 60-second hard ceiling.
   `no-lmr-long` gives the incomplete depth-4 interior-LMR ablation 60 seconds.
4. `forced174`, `forced174-unpromoted`, and `forced176`: independent candidate
   scores and PV traces under baseline and half-tempo. The adaptive `mechanism175`
   phase uses `corpus175.json` and compares Black's Gold capture with its Tengu
   capture, at depth 2. It is a mechanism probe in the development game, not a
   held-out position.
5. `teacher-forced-*` and `teacher-full-*`: independent SEEDS2 opinions at the
   original depths and one deeper, using the reconstructed checkpoint under
   `teachers/`. These are another engine's opinions, not optimal-move labels.
6. `timed-hard`: six settings, all 12 frozen positions, three shuffled 3-second
   searches each, with `--no-soft-stop`. `timed-production` repeats the two
   targets with the ordinary CLI's predictive stopping enabled. The additional
   `timed-zero-hard` and `timed-zero-production` phases test global zero-tempo in
   the same way. **Ordinary CLI stopping is not the live tournament's Fischer
   clock policy**; the probe has no tournament clock balance or Fischer soft
   allowance.
7. `teacher-control-*`: after selecting modal moves from the timed trials, score
   each distinct choice on the four differing half-tempo control boards with
   SEEDS2 at depths 3/4 and a five-second limit. `control-teacher-selection.json`
   was frozen before those teacher scores. Zero-tempo adds three new routes in
   those same four boards, recorded before scoring in
   `control-teacher-zero-selection.json`; its changes on four other boards remain
   unjudged. Modal ties are resolved lexicographically.

For example, reproduce the repeated strict-depth phase with the common binary,
data-root, and CPU arguments from above, then:

```sh
python3 benchmarks/nnue_tactics/slot0673-variants/run.py run \
  --binary /absolute/path/to/tactical_probe --data-root /home/frank/taikyoku_shogi \
  --variants baseline,tempo_50,q_tempo_50,q_tt_context,major_capture_required \
  --group target --mode fixed --no-soft-stop --repetitions 3 \
  --output /absolute/path/to/results/fixed-repeats

python3 benchmarks/nnue_tactics/slot0673-variants/run.py run \
  --binary /absolute/path/to/tactical_probe --data-root /home/frank/taikyoku_shogi \
  --variants-file benchmarks/nnue_tactics/slot0673-variants/timed-variants.json \
  --group all --mode timed --no-soft-stop --repetitions 3 \
  --output /absolute/path/to/results/timed-hard

python3 benchmarks/nnue_tactics/slot0673-variants/run.py run \
  --binary /absolute/path/to/tactical_probe --data-root /home/frank/taikyoku_shogi \
  --model benchmarks/nnue_tactics/slot0673-variants/teachers/SEEDS2.json \
  --variants-file benchmarks/nnue_tactics/slot0673-variants/teacher174.json \
  --positions target174 --depth 4 --no-soft-stop --dump-pv \
  --output /absolute/path/to/results/teacher-forced-174-d4

python3 benchmarks/nnue_tactics/slot0673-variants/summarize_results.py
```

Use a new output directory for a different build or configuration. The initial
screen and later strict-depth phases used two binary revisions; the later build
adds the explicit predictive-stop override and tests. Final CLI hardening pins q
depth against inherited environment settings; emitted earlier configs confirm
the measured q depths already matched the intended settings.
