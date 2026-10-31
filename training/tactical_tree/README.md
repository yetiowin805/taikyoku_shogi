# Tactical-tree pilot

This isolated experiment compares ways to investigate a recorded evaluation swing.
It does not change tournament search, model weights, sample selection, or training data.
The initial corpus is frozen in `benchmarks/tactical_tree/20261017/corpus.json`:
six historical diagnostic/control episodes and six recent episodes, including a
1,000-plus-ply prefix, NNUE/NNUE and handcrafted/handcrafted games, and mate recognition.
Selection precedes comparative results. Previously reserved slots 672/769/867 are not used.

## Matched-compute follow-up (October 18)

`prepare.py --comparison --leader-run RUN` enables the new comparison. It follows the
highest-rated active agent in the published order-neutral standings, with no required
rating margin. At the first dispatch of each episode, the supervisor freezes that
leader's manifest and network, the rating snapshot, and the input hashes. All three
arms and the follow-up checks for that episode use this same model. A later episode
can use a new leader; restarting a partial episode keeps its original binding. There
is no designated older NNUE or handcrafted judge. Component ratings remain provisional
when the graph is disconnected, just as in the displayed standings.

The follow-up uses six existing diagnostic episodes and six additional episodes from
different games. `select_corpus.py` selects the latter by a seeded hash of saved-score
candidates: early and middle finite swings, a 1,000-plus-ply history, mate onset, an
NNUE/NNUE crossing, and a quiet control. It excludes every previous pilot game and
the reserved slots. Freeze the corpus before running any arm, and report the diagnostic
and fresh cohorts separately. These twelve cases are a preliminary comparison, not
a representative estimate of tournament strength.

The three arms are:

- **uniform:** deepen the nine recorded positions round-robin. Recommendations come
  only from each root's deepest completed search. There is no branch backup or extra
  coverage step hidden in this baseline.
- **adaptive:** search the same nine roots, choosing where to deepen using bounded
  odd/even instability, candidate separation, and previous search count. Recommendations
  still come directly from root searches.
- **branches:** use the existing contested-branch policy and partial minimax backup,
  including its fair legal-edge coverage step.

All start with depth-three searches of the recorded sequence. Checkpoints are at 60,
180, and 420 **CPU seconds**, counting the driver, model loading/replay, generation,
and the worker's user/system CPU. Waiting for a CPU or disk is not analysis compute.
Individual requests use the existing 3–30-second search settings. An active request
finishes normally when a checkpoint is crossed; its later results cannot be credited
to an earlier checkpoint. Snapshots retain the last available recommendation and its
actual CPU time at or before each threshold, alongside the time the crossing was
observed. Report actual overshoot and unused checkpoint time, too.

Completed iterations are immediately durable. Explicit interruption resumes the tree
and records a resume count. After a hard process kill, CPU consumed since the last
checkpoint cannot be recovered precisely: resumed arms must be identified separately
from uninterrupted primary timing comparisons. There is no overall elapsed-time kill,
startup watchdog, or deadline that drops unfinished audits. Finishing the finite planned
comparison returns its CPUs; further experiments can use later checkpoints.

Audit locations are fixed **before** results: two plies before the anchor, the anchor,
and two plies after it. At every location, pool the played move, all arms' proposals
at all three checkpoints, and a fresh root-search proposal. Cache identical full routes
within this frozen episode, then:

1. Search every resulting child toward depth six for up to 25 seconds, with a fresh
   search workspace. Compare against the uniform arm at matching odd and even depths.
2. Advance every unique proposal with depth-three searches (up to six seconds each)
   and inspect endpoints after four and eight replies. Compare endpoint scores only
   at the deepest completed depth common to the two endpoints. Record shallower steps,
   the entire continuation, and all completed search iterations.

Missing recommendations or horizons remain missing. Actual terminal outcomes are
reported separately from numerical/mate-range search scores. Identical proposals
reuse the same audit, avoiding different continuations for the same choice. Report
move changes, gains and regressions, parity disagreements, both rollout horizons,
CPU cost and missing coverage. `report.py` writes individual rows and equal-case
aggregates; inspect raw trajectories before drawing conclusions from a mean score gain.

This experiment asks whether exploration finds recommendations better supported by
additional search and forward play than ordinary deepening at the same compute. It
does **not** establish the true blunder location or an Elo gain. Follow-up work shares
the strongest model's blind spots; using the same model keeps the comparison focused
on the search strategy. Useful results would justify a later data/training experiment,
followed by actual games at the target time control. No hypothetical labels enter
training and no new network is trained here.

Expect roughly 3–6 hours with two available CPUs for twelve episodes and full audits;
this is an estimate, never a shutdown timer. The shared supervisor lends at most two
existing CPU leases, never interrupts a game, and lets its current jobs finish before
returning leases when admission or coordinator maintenance holds the allocation gate.
It resumes automatically when that gate reopens. Production engines, models, and
collector policy are unchanged.

Example (use a separate output directory from the original pilot):

```sh
python3 training/tactical_tree/select_corpus.py --run "$RUN" \
  --previous benchmarks/tactical_tree/20261017/corpus.json --out corpus.json
python3 training/tactical_tree/prepare.py --comparison --leader-run "$RUN" \
  --corpus corpus.json --game-dir "$RUN" --binary target/release/tactical_tree_worker \
  --out results --control "$RUN/analysis" --admission-status "$ADMISSION_STATUS" \
  --build BUILD.json --config config.json
python3 training/tactical_tree/supervise.py config.json
python3 training/tactical_tree/report.py results
```

## Four exploration policies

Every policy first searches the same nine positions around the episode (depth three,
up to three seconds per position), then explores until the next 420-second comparison
checkpoint. Checkpoints occur between searches; a search already in progress finishes
under its own search settings before the next comparison task starts:

- **local:** deepen the recorded sequence round-robin. This is the simple baseline.
- **pv:** follow the currently preferred continuation, searching each new position independently.
- **contested:** emphasize close alternatives, odd/even score disagreement, and changes caused
  by information from descendants; alternate deepening and branching.
- **refutation:** combine contested search with cheap nominations that move a piece away from
  the recorded reply, capture its mover, or block its straight corridor. These are nominations,
  not proof that the reply is refuted. The opponent is always free to choose another legal move.

Every tenth action in every policy expands the first unseen legal edge breadth-first.
Full move routes, promotion and intermediate squares are retained. This slow coverage
step prevents a heuristic score from permanently excluding a move. Each action is a
finite search; unsuccessful deepening gets progressively more time, up to 30 seconds
for this pilot. No production pruning or quiescence setting is changed.

## Estimates and proofs are different

All stored scores are from Black's perspective. Backup maximizes for Black and minimizes
for White. An independently searched child replaces its parent's old estimate for that
move. Other root candidates remain heuristic placeholders; selective-search root lines
are not guaranteed exact values. This permits revising an ancestor's preferred move
without claiming that the partial tree is solved.

A separate WDL interval starts at [-1,+1] for every unexpanded edge. Only actual terminal
positions narrow it: the tournament's fivefold repetition, progress/insufficient-material
draw rules, royal capture, or no legal move. Search's twofold repetition and mate-range
scores never establish a proof. Full inherited history is replayed, not just a board snapshot.

With unlimited resources, exhaustive fair expansion can solve a finite game tree under
the encoded rules. The finite pilot cannot promise this. A numeric evaluation jump is not
necessarily a change in theoretical WDL, and the largest backed-up regret is only a
suspected blunder. Expanding beyond the initial window would be a separate follow-up if
the cause appears to precede it.

This distinction follows the completion requirement in [Cohen-Solal, 2021](https://arxiv.org/abs/2109.09468).
Best-first minimax and neural evaluation have been studied by
[Korf and Chickering](https://www.maxchickering.com/publications/aij96.pdf) and
[Cohen-Solal and Cazenave](https://www.lamsade.dauphine.fr/~cazenave/papers/MinimaxStrikesBack.pdf).
These motivate the policy families, not a claim that our priority formula is established or optimal.

## Common audits, including playing forward

After all four policies finish their comparison interval, complete a common audit.
There is no elapsed-time cutoff on the audit or the overall experiment:

1. Pool proposals at the original anchor and the two most nominated suspected blunders.
   Independently search these roots and the positions after their candidate moves,
   targeting depth six with 25–30-second action budgets. Report achieved depth. Compare
   candidate versus played move at identical completed depths, retaining deepest odd
   and even results separately. A gain of at least 1,000 on both parities is a *corroborated
   improvement*, not ground truth. Unchecked proposals remain unchecked, not failures.
2. Play forward at the most nominated location, chosen before
   the audit scores are seen. Pool the played move, each policy's preferred move, and the
   fresh root choice. Advance all unique candidates round-robin, aiming for depth three
   with up to six seconds per move. Keep the score trajectory and compare after **four
   and eight further plies following the nominated move**. Both endpoints have the same
   side-to-move parity across candidates. Evaluate endpoints by search, not bare static eval.
   Endpoint searches also supply the move for continuing to the longer horizon.

Timeouts may leave shallower continuations; `shallow_steps` counts them, and endpoint
scores are compared only at matching completed depths unless one is an actual terminal
outcome. Incomplete horizons are missing, never assigned zero. The original game's
winner is never attached to a hypothetical line. If a rollout blunders, that is evidence
about this continuation policy, not proof that the initial choice was bad. Inspect both
horizons, parity scores, and trajectories rather than choosing whichever endpoint looks best.

Use these results to compare reproducibility of suspected blunder location, corroboration
of proposed alternatives, stability as time increases, disagreement between audits, and
cost. With 12 deliberately varied episodes and one evaluator (512v4), conclusions remain
preliminary. The common audit is independent work, but shares the engine's systematic
biases. A second teacher and untouched episodes are useful follow-ups. No labels from this
pilot enter training automatically.

## Protocol, durability, and CPU ownership

Build `cargo build --release --bin tactical_tree_worker`. The worker accepts
`GAME START_PLY MODEL`, loads the original checkpoint, and serves version-one JSONL:

```json
{"version":1,"id":"request-1","op":"search","path":[],"depth":3,"time_ms":3000}
```

`path` contains full serialized `Move` objects. `inspect` returns all legal routes,
history/draw/repetition information and optional legality of a nominated response.
`search` emits completed `iteration` events immediately, followed by `complete` or `error`.
Each request starts with fresh production search state. The supervisor detects process
exit and protocol errors. Neither startup nor a request has an external elapsed-time
watchdog that kills the worker. Individual timed-search settings control search effort;
they do not terminate the experiment. Explicit cancellation remains supported.

Completed iterations and tree updates are fsynced immediately. `tree.json`, `events.jsonl`,
`summary.json`, and `audit/audit.json` retain paths, achieved depths, scores, candidates,
node counts, timing and errors. Resume verifies game, model manifest/blob, binary and
Python source hashes. A larger `run.py --seconds` continues an existing tree; previous
snapshots remain available. To run modified code, use a new output directory/configuration.
For an orchestration-only upgrade, preserve the original immutable inputs and additionally
verify `runner_identities` for the new runner. `runner_history` records when it takes over;
old completed iterations and their provenance remain intact.

The supervisor uses the live coordinator's existing `cpu-N.lock` files. Blocking waiters
queue behind games, then lend at most two of CPUs 4–7. CPUs 0 and 2 remain the tournament
and champion workers; this deployment's trainer owns CPUs 1 and 3. The pilot never writes
`analysis-demand.json`. CPU leases are inherited through job and helper processes, and
children are pinned to their leased CPU. The pilot yields for model admission. Disk space
below 4 GiB pauses work. Store outputs on the root disk, not the training volume.

The initial work estimate is roughly eight CPU-hours, depending on audit branching and
search speed. This is an estimate, never a stop condition. Waiting for games can add wall
time, and one available CPU takes longer. The supervisor works through every planned
comparison and complete audit, then returns its CPUs. No overall wall-time budget or
automatic elapsed-time shutdown exists. All checkpoints remain resumable.

## Reproduce / deploy

Create `BUILD.json` with the git revision, `rustc -Vv`, build command, and platform.
Keep code/binary immutable in a separate deployment directory. Prepare validates inputs
before any worker launches (game-dir contains the original completed game files):

```sh
python3 training/tactical_tree/prepare.py \
  --corpus benchmarks/tactical_tree/20261017/corpus.json \
  --binary target/release/tactical_tree_worker \
  --model /opt/nnue-v4-20261003/NNUE_W512_v4/model.json \
  --game-dir /opt/taikyoku_shogi/data/raw/tourney/royal-nnue-v3-32-20260925 \
  --control /opt/taikyoku_shogi/data/raw/tourney/royal-nnue-v3-32-20260925/analysis \
  --admission-status /mnt/HC_Volume_107032879/taikyoku/runs/v5-20261017-window23/admission/status.json \
  --build BUILD.json --out /opt/tactical-tree-pilot-20261017/results \
  --config /opt/tactical-tree-pilot-20261017/config.json
python3 training/tactical_tree/supervise.py /opt/tactical-tree-pilot-20261017/config.json
python3 training/tactical_tree/report.py /opt/tactical-tree-pilot-20261017/results
```

Use a dedicated systemd service with `KillMode=control-group` for unattended operation.
Stopping that service checkpoints the pilot and returns its CPUs; it does not stop the
tournament or trainer. No tournament binary is replaced. Tests:

```sh
cargo test --bin tactical_tree_worker
python3 -m unittest discover -s training/tactical_tree -p 'test_*.py'
```
