# Winning handcrafted teachers

The dual-label sidecar now samples handcrafted wins over NNUE for analysis by
that winning agent plus 2048 v3, with the existing 30-second budget per teacher.
Other games retain 512 v2 plus 2048 v3. This collects alternative labels; it does
not decide the winner's evaluation is correct or retrain a model.

For each eligible game, `winner-teacher-v1` keeps at most ten positions:

- Up to two episodes on the losing side's turns: a finite score worsening by
  at least 1,000 units, or a transition from a finite score to a losing mate score.
  Scores are Black-absolute and converted to the losing side's perspective.
- Mate transitions rank first, then larger drops. Episode centers must be at
  least 16 plies apart. Before each episode, retain positions 8, 4, and 2 plies
  earlier, on the loser's turns. These are positions before its moves, not after
  them. Clip at game boundaries and exclude missing and mate-range source scores.
- Up to four deterministic representative samples, one per progress quartile,
  at least four plies from already selected positions.

This bounds the targeted material at six positions per game, deduplicates shared
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
plus at most ten new ones. Fresh handcrafted-win games use only the new sampler.

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
