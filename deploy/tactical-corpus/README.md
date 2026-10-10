# Tactical score-drop corpus

This mines candidate episodes from completed loss replays. A large drop is a
discovery signal, **not a solved tactic** or a ground-truth move label.

Build `cargo build --release --bin tactical_replay`, then run:

```sh
python3 deploy/mine_tactical_moments.py \
  --inventory /path/to/inventory.json \
  --games /path/to/replays \
  --replay target/release/tactical_replay \
  --output /path/to/corpus
python3 -m unittest discover -s deploy -p test_mine_tactical_moments.py
```

The inventory is a JSON object with `captured`, `completed_games`, `run_id`, and
`losses`. Each loss supplies `slot`, `agent`, `opponent`, `agent_color` (`Black` or
`White`), and `path` (the original replay path; its basename is resolved under
`--games`). Additional provenance fields are retained.

## Selection

- Compare only the target agent's decisions, converting black-absolute scores
  to that agent's perspective. These are internal score units, not calibrated
  centipawns or win probabilities.
- Detect drops of at least 1,200 units over one to three own decisions. The
  threshold is configurable. Do not cross absent scores, depth-zero fallbacks,
  or scores of magnitude 100,000 or more (mate/terminal-like scores).
- Prefer persistent drops and positive-to-negative crossings, but retain
  rebound and already-losing examples with explicit flags.
- Also compare the low score with the most recent own decision at the same
  completed depth (up to six own decisions back). Flag depth-confounded drops
  when a depth change lacks a corroborating same-depth decline. Alternation
  between depth 3 and 4 can otherwise masquerade as repeated tactical failures.
- Suppress nearby recognition points within six GUI plies. Context windows can
  still overlap; they are not all statistically independent.
- Save earlier decision positions, the recognition point, and twelve subsequent
  GUI plies. Record `record_move_number` separately: two-step moves make it
  differ from the GUI's 1-based replay ply.
- Deduplicate identical decision positions for the same model. Group games
  sharing any selected exact position before reserving approximately a quarter
  of games as holdout. Known investigated games 673 and 817 stay in development.
  This prevents exact selected-position leakage, not every near-duplicate
  opening or structural motif.
- Select one stable-score control per game when available.

## Outputs

`corpus.json` contains all candidates, provenance, source SHA-256s, records of
actual captures, score/depth/time metadata, duplicate links, and split labels.
`positions.jsonl` stores unique TSFEN snapshots and their game/ply occurrences.
`trajectories.json` keeps each agent's full score history. `replays/` contains
exact engine replay events; `review.html` is a dependency-free expandable index.

Replay removal lists include friendly pieces removed by sweeping moves. Material
deltas use the engine's seed material scale and are descriptive, not NNUE scores.
Source games are retained intact. A position at ply N is **before move N**.
Use the original replay for analysis: TSFEN does not preserve repetition history.
All cases start as `unvalidated_candidate`; a loss later in the observed line
does not establish that an earlier mistake was avoidable.

## Before using as a pass/fail benchmark

1. Inspect the observed continuation and move backward to the last plausible
   defensive decision. A recognition point can already be too late.
2. Compare baseline and experimental searches at both equal completed depth and
   equal work/time. Mark incomplete searches; never equate their final scores.
3. Force promising alternatives and inspect the opponent's best reply, using a
   deeper independent reference where feasible. A higher score from the same
   evaluator is not proof of a better move.
4. Store an accepted move set or tactical invariant (e.g. avoid an unrecaptured
   major-piece loss), with evidence and confidence. Avoid requiring one arbitrary
   move when several defenses work.
5. Freeze the adjudicated development set, then run the untouched holdout and
   stable controls. Use games/position families as sampling units, not every
   overlapping window as an independent success.

No search behavior is changed by these tools.
