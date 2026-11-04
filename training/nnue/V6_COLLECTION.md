# V6 evidence collector

`v6_collect.py` implements the collection stages proposed in the
[v6 research report PR](https://github.com/yetiowin805/taikyoku_shogi/pull/211).
It does not train, tune material, admit networks, or change the tournament.
Install it in a separate frozen deployment directory.

## Prepare and launch

Build `tactical_tree_worker` from the same source tree as the Python modules.
Recover the sample indexes covering both parents and their inherited training
exposure. Use independent copies in the collection directory. Full feature
arrays are unnecessary for these canonical-hash exposure checks.

```sh
python3 training/nnue/v6_collect.py prepare \
  --out /absolute/new-collection \
  --run /absolute/corrected-rule-run \
  --source-repo /absolute/original-repository \
  --binary /absolute/frozen/tactical_tree_worker \
  --parent-samples /absolute/v45/samples.jsonl \
  --parent-samples /absolute/v3/samples.jsonl \
  --lease-index 7 --revision FULL_COMMIT_SHA
python3 training/nnue/v6_collect.py run /absolute/new-collection/config.json
```

Preparation freezes the completed-game roster, initial teacher copies and code/binary
hashes. Teacher selection follows the existing current order-neutral standings:
use the highest-rated active entrant, retaining the incumbent on exact ties,
with no promotion margin. The audit challenger is the highest-rated entrant
with a different NNUE blob (or different handcrafted evaluator); the diagnostic
is the highest-rated handcrafted entrant. Training parents do not select teachers.

Standings are checked at most once per minute when binding a new root. Each
root, its children and every later search budget keep that original teacher
set. Changes apply to new roots only; completed evidence is never reinterpreted.
The catalogue stores the exact ranking snapshot and immutable model identities
for every binding. Invalid/stale fits retain the previous verified set and
report an error in `teacher-status.json`. A selected entrant requiring a
historical engine cannot silently run under the current worker.

The runner copies complete game records and queues for the selected
flexible worker's existing CPU lease, allowing its current game to finish.
It pins itself and its single engine child to that worker's actual CPU ID and
passes the lease descriptor to the child. It yields between requests when
maintenance holds the allocation lock. Workers 0 and 2 cannot be selected.

For a durable service, install `deploy/systemd/taikyoku-nnue-v6.service` and create
`/etc/taikyoku/nnue-v6.env` with absolute `COLLECTOR` and `CONFIG` paths. The unit
adds a one-CPU cgroup quota. Enable/start only after preparation succeeds, then
verify both the service state and advancing committed rows. Explicit failures
do not automatically restart; abnormal termination can resume committed work.
Stopping kills the process group and releases the CPU lease.

## Collection stages

- Up to 32 representative and 8 challenge roots per game. Representative roots
  span progress and color with eight-ply spacing. At least two challenge slots
  are random. Remaining training challenges use saved static/search gaps,
  same-side swings, promotions and outcome discrepancies as selection proxies.
  Holdout challenges are random. At most one explicit mate-claim challenge is
  selected per game. There is no special range two-mover oversampling.
- First, up to 1,000 representative training roots, generally one per game, get
  primary 10-second searches. Up to 100 get challenger and handcrafted
  searches too. The pilot reports depth coverage, usable labels and q timeouts.
  Its gate catches operational failure; passing does not establish teacher
  strength or adequate search depth. Teacher identity stays fixed per root family.
- Remaining broad labels use the primary teacher. Then 6,000 roots get both
  teachers at 30 seconds. Half the audit is random; the rest targets fresh
  discrepancies in training groups, with at most eight roots from one game.
- Up to 2,000 training roots produce three legal child positions: primary best,
  a static favorite from a bounded candidate pool, and another alternative.
  Root-line scores are heuristic; each child gets an independent scalar search.
  Children inherit the root's group/split and receive no game outcome.
- Two children from up to 1,000 branch roots get paired 30-second verification;
  two from up to 200 roots also get paired 120-second verification. Another
  1,600 ordinary roots get paired deep searches, selected from the root audit.
- Final game positions receive independent terminal checks. Saved outcomes stay
  ineligible unless the replayed rule-terminal result agrees. Nonterminal
  timeouts, move limits and unknown adjudications do not become outcome labels.

Fresh searches preserve the selected entrant's checkpoint search policy, including
its experiment, trial and q settings. Only depth/time limits and irrelevant clock/trace
controls are overridden; both the base policy and overrides are recorded. Scores remain
Black-absolute, with side to move recorded separately. Each scalar job records
static/material scores, a bounded standalone quiescence probe, completed
iterations, final telemetry, model/binary hashes, budgets and full-route moves.
Standalone q starts with no previous landing and differs from the production
AB leaf gate; timed-out probes are marked aborted. Only fully completed root
iterations become labels. Partial-root replacements and root-line scores do not.

The nominal plan is approximately 460 CPU-hours at 1,600 games, plus pilot
challengers, replay, q probes, model loading, preparation and outcome checks.
It is a multiweek job on one CPU. Actual counts may be smaller because of
spacing, short games, terminal roots and unavailable alternatives.

## Splits, recovery and outputs

Related pairs, exact non-opening starts, substantial prefixes and inherited
parent groups stay together. New groups use an 80/10/10 hash split; conflicting
inherited splits quarantine the group. Canonical hashes match the existing
Python feature-duplicate encoding. Parent-training exposure or cross-split
duplicates revoke fresh holdout eligibility. State/history hashes remain
separate because identical evaluator inputs can have different search values.

This is raw evidence, not a training-ready manifest. Subsequent preparation must
resolve duplicate/history conflicts, calibrate probability targets, verify mate
claims, select stable ranking pairs and implement draw weights. The collector
never certifies a mate from its score alone. Holdout eligibility is provisional
until all stages finish. Verified outcome eligibility is stored on game rows;
position/branch rows deliberately do not inherit it automatically.

- `config.json`, `source-state.json`, `games/`, `models/`: frozen inputs.
- `catalogue.sqlite`: games, positions, exposure, jobs and iteration checkpoints.
- `status.json`: active job, CPU, stages and committed counts.
- `pilot-summary.json`: early operational quality audit.
- `teacher-status.json`: current standings selection or retained-teacher error.
- Catalogue `teacher_root:*` / `teacher_epoch:*` metadata: permanent family bindings and ranking snapshots.
- `engine.log` and the service journal: diagnostics and errors.

SQLite commits preserve completed jobs. Interrupted work is retried, retaining
iteration evidence without declaring the job complete. Ordinary failures get
at most two attempts and remain explicit afterward. Final success requires all
scheduled jobs to complete. A 900-second watchdog catches stuck workers,
separately from search budgets. A free-space floor stops optional writes. Stop
and restart to resume; do not edit the frozen config or delete failure records.

```sh
python3 training/nnue/v6_collect.py status /absolute/new-collection/config.json
python3 -m unittest discover -s training/nnue -p test_v6_collection.py
cargo test --bin tactical_tree_worker --test v6_collection
```
