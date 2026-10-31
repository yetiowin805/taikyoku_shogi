# NNUE parity survey

This isolated survey compares all available v4/v4.5/v5 tournament checkpoints
on common complete-history positions. It changes only the experimental oracle
and orchestration; tournament search and training remain unchanged.

`parity_prepare.py` freezes 12 distinct completed games from slots >=1300 involving
v4/v5 agents, selected by a seeded hash. It chooses one position per game, four
each from early (5–25%), middle (35–65%), and late (75–95%) portions. Selection
does not inspect evaluation scores. Previous tactical cases and reserved slots
are excluded. This is a small stage-stratified survey of recent games, not an
unbiased sample of all tournament positions or a precise prevalence estimate.
Slots 1337/1024 and 673/174,176 form a separate diagnostic cohort.

All positions receive independent, fresh-workspace searches at depths 1–4.
The first selected case in each phase and all diagnostics also receive depth 5.
The optional `fixed_depth` protocol flag removes the normal search time limit;
there is no external deadline. Every completed iteration is fsynced immediately,
and finished requests are reused on restart. Each job verifies game/model hashes.
The original immutable NNUE blobs are referenced rather than copied; manifests,
game records and all source/binary identities are frozen before dispatch.
The same saved search defaults are required for all models. Search environment
overrides are removed. Width is reported as a confounder of generation comparisons.

Record scores from Black's perspective, full move routes, all root iteration
events, requested and completed depth, elapsed time, cumulative nodes and q nodes.
Primary summaries include adjacent-depth gaps, same-parity changes, changed moves,
and `S3 - (S2+S4)/2`, which removes a linear depth trend when estimating the local
odd/even offset. Mate-range estimates are separate from finite-score statistics.

Cheap auxiliary probes evaluate the exact board with either turn, and inspect
the legal first continuations selected at depths 3 and 4. A same-board turn gap
can be genuine and is not itself evidence of an evaluation error. These are not
the actual terminal search leaves. Standalone q-depth-2 probes use the existing
`probe_quiescence` harness, with no previous landing and its own capture-policy
defaults; they do not reproduce the context-dependent production AB leaf gate.
Additional leaf instrumentation is a later diagnostic if these probes are insufficient.

The shared supervisor borrows only CPU leases 4 and 5, waiting for current games
to finish, and yields at job boundaries for allocation maintenance. No game is
killed, no analysis demand is rewritten, and no production checkpoint is replaced.
An explicit operator interruption remains possible; health/disk failures are
reported rather than silently counted as successful samples.

Reproduce with a fresh output directory:

```sh
cargo build --release --bin tactical_tree_worker
python3 training/tactical_tree/parity_prepare.py --run "$RUN" \
  --previous /opt/tactical-comparison-20261018/config.json \
  --out "$OUT/results" --binary "$OUT/bin/tactical_tree_worker" \
  --build "$OUT/BUILD.json" --config "$OUT/config.json" \
  --admission-status "$ADMISSION_STATUS"
python3 training/tactical_tree/supervise.py "$OUT/config.json"
```

Keep private inputs/results off GitHub. Freeze source revision, compiler and binary
hash in `BUILD.json`. The normal one-position/timed protocol remains compatible.
