# Tactical-tree pilot

This isolated experiment compares ways to investigate a recorded evaluation swing.
It does not change tournament search, model weights, sample selection, or training data.
The initial corpus is frozen in `benchmarks/tactical_tree/20261017/corpus.json`:
six historical diagnostic/control episodes and six recent episodes, including a
1,000-plus-ply prefix, NNUE/NNUE and handcrafted/handcrafted games, and mate recognition.
Selection precedes comparative results. Previously reserved slots 672/769/867 are not used.

## Four exploration policies

Every policy first searches the same nine positions around the episode (depth three,
up to three seconds per position), then spends the remainder of its 420-second budget:

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

After all four policies finish, spend up to 540 seconds per episode on a common audit:

1. Pool proposals at the original anchor and the two most nominated suspected blunders.
   Independently search these roots and the positions after their candidate moves,
   targeting depth six with 25–30-second action budgets. Report achieved depth. Compare
   candidate versus played move at identical completed depths, retaining deepest odd
   and even results separately. A gain of at least 1,000 on both parities is a *corroborated
   improvement*, not ground truth. Unchecked proposals remain unchecked, not failures.
2. Reserve 300 seconds for forward play at the most nominated location, chosen before
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
Each request starts with fresh production search state. A worker is supervised and
terminated if it wedges beyond twice its requested action time plus ten seconds
(minimum 30 seconds). This is a per-action experiment safeguard, not a training limit.

Completed iterations and tree updates are fsynced immediately. `tree.json`, `events.jsonl`,
`summary.json`, and `audit/audit.json` retain paths, achieved depths, scores, candidates,
node counts, timing and errors. Resume verifies game, model manifest/blob, binary and
Python source hashes. A larger `run.py --seconds` continues an existing tree; previous
snapshots remain available. To run modified code, use a new output directory/configuration.

The supervisor uses the live coordinator's existing `cpu-N.lock` files. Blocking waiters
queue behind games, then lend at most two of CPUs 4–7. CPUs 0 and 2 remain the tournament
and champion workers; this deployment's trainer owns CPUs 1 and 3. The pilot never writes
`analysis-demand.json`. CPU leases are inherited through job and helper processes, and
children are pinned to their leased CPU. The pilot yields for model admission. Disk space
below 4 GiB pauses work. Store outputs on the root disk, not the training volume.

The planned work is about 7.4 CPU-hours (roughly 3.7 hours on two CPUs plus overhead).
A 4.5-hour pilot budget starts when the first CPU is acquired; unfinished work is saved
and leases released. Waiting for games can add wall time, and one available CPU will
complete less work. This is the requested preliminary experiment budget, not a limit
on training or future investigations. All checkpoints remain resumable.

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
