# Winning handcrafted teachers

The dual-label sidecar now samples handcrafted wins over NNUE for analysis by
that winning agent plus 2048 v3, with the existing 30-second budget per teacher.
Other games retain 512 v2 plus 2048 v3. This collects alternative labels; it does
not decide the winner's evaluation is correct or retrain a model.

For each eligible game, `winner-teacher-v2` keeps at most 32 positions:

- Up to four episodes on the losing side's turns: a finite score worsening by
  at least 1,000 units, or a transition from a finite score to a losing mate score.
  Scores are Black-absolute and converted to the losing side's perspective.
- Mate transitions rank first, then larger drops. Episode centers must be at
  least 16 plies apart. Before each episode, retain positions 16, 8, 4, and 2 plies
  earlier, on the loser's turns. These are positions before its moves, not after
  them. Clip at game boundaries and exclude missing and mate-range source scores.
- Up to sixteen deterministic representative samples, one per equal-progress stratum,
  at least four plies from already selected positions.

This bounds the targeted material at sixteen positions per game, deduplicates shared
positions, and adds ordinary context. It does not prove a sampled mistake caused
the loss. Related-opening train/validation grouping matches the existing sampler.
Sampling policy, episode, offset, quota and teacher identities remain in records
so a future training run can control their weight. Mate-range search outputs are
preserved; existing training filters may exclude them until explicit mate-target
handling is added.

A separate policy-versioned scan ledger backfills already completed games once.
At an already sampled game/ply, reuse the existing moment ID and requeue only that
position. Preserve replaced 512 labels in `previous_teacher_searches` and in the
search cache; reuse existing 2048 labels with matching budget. Existing unselected
legacy moments remain intact, so older games can retain their original samples
plus at most 32 new ones. Fresh handcrafted-win games use only the new sampler.

Teacher selection uses frozen checkpoint contents, not agent names. Original
historical engine bindings are retained when present; modern winning checkpoints
use the active compatible analysis build. Both teachers retain independent
results, model hashes, helper hashes and immediate per-search persistence. Failed
positions are visible in `dual_failures`; restarting resumes incomplete work.

Deployment only requires restarting `taikyoku-dual-labels.service`; do not restart
the tournament supervisor. The existing CPU-3 request/lease protocol still waits
for any shared game to finish and returns the CPU when the queue empties. Status
adds `winner_selected` and `winner_completed`. Backfilled winner samples are
processed before the normal queue. For rollback restore the prior sidecar code;
completed catalogue records and cached searches should be retained.

Validation: `python3 -m unittest discover -s deploy -p 'test_*.py'`.

Version 2 expands the original ten-position policy. The versioned ledger revisits
previously scanned games, retaining overlapping IDs and cached teacher searches.
Previously collected positions remain valid, including those outside the new
selection. Training should normalize game/episode contributions rather than
assigning extra weight solely because more correlated labels were collected.

## Replacing the default second teacher

Use `dual_label_sidecar.py replace-teacher --run-dir RUN --model MODEL --check-only`
to validate and preview. Stop only the dual-label service, run without
`--check-only`, and restart that service. The command takes the collector lock,
backs up configuration and SQLite, snapshots the checkpoint and records an
idempotent backfill manifest. Game workers and coordinator keep running.

SEEDS2 replaces the second slot; the first remains 512v2 or the winning
handcrafted teacher. Equal checkpoint identities share one search. Replaced
2048v3 labels are archived, including if the next search is interrupted.
Unselected completed records retain their original valid teacher identities.

Backfill keeps up to eight completed positions per game: four emphasizing
pre-mate samples and teacher disagreement, spaced eight plies apart, plus four
spread across the existing position list, spaced four plies from selections.
All outcomes are included. Pending positions use the new teacher automatically;
future sampling still uses the existing 32-position winner policy. Restart
resumes the backfill without requeuing already refreshed positions.

## Automatic order-neutral champion

Before each position (without interrupting an in-flight search), the collector
checks for new completed game results. With the collector stopped, enable the rule using:

```
python3 deploy/dual_label_sidecar.py enable-champion --run-dir RUN --ratings-bin /absolute/path/tournament_ratings
```

The read-only `tournament_ratings` helper comes from the order-neutral ratings
implementation (PR #135). Its binary SHA is pinned in the configuration. The
collector fits an immutable state snapshot and selects the global rating leader
only if its rating is at least 50 points above the incumbent teacher. It keeps
that incumbent between switches (not a historical peak rating). All completed
games have equal weight; inactive or disconnected fits never trigger a switch.
No extra confidence/significance threshold is imposed.

Each promotion verifies the frozen model and any historical engine binding,
backs up the catalogue, preserves older labels, and queues the same bounded
backfill. Pending work uses the current teacher. A unique refresh generation
allows a previously used teacher to return safely. The original 512v2/winning
handcrafted first-teacher policy is unchanged. Current decisions/errors live in
`analysis/champion-status.json`; switches are appended to
`analysis/champion-switches.jsonl`. Errors leave analysis running on the current
teacher and retry after at least 60 seconds. No game workers are restarted.
