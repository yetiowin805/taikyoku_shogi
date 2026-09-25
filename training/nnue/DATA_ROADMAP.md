# NNUE data quality and mate-position roadmap

Recorded from the September 24–25, 2026 discussion. This is a research roadmap,
not a change to the running training job, analyzer, search, or tournament.
Numbers below describe that snapshot; they are not live status.

The objective is stronger play at the tournament's search budget. Better fit to
teacher evaluations is useful evidence, but does not by itself establish stronger
play. Proposed sampling percentages and budgets are starting points for experiments,
not measured optima.

Related documents: [pilot recipe](PILOT.md), [generation v3](GENERATION.md), and
[training implementation guide](README.md).

## 1. What motivates this work

The working hypothesis is that royal exposure, the mobility of a few powerful
pieces, and their combined attacks frequently decide games in ways that material
alone describes poorly. This comes from examining games; we have not established
how weakly material and winning chances correlate across the whole population.
The prevalence of mating finishes alone does not establish that material is
unimportant: material can help create or prevent the eventual attack.

Our architecture can already learn to override material: evaluation is fixed
material plus a learned positional correction. We need examples where those two
terms should disagree. A network should recognize a dangerous position before the
short game search can see the terminal outcome, while distinguishing a dangerous
but defensible royal from one that cannot be saved.

Snapshot of the current data setup:

- The generation-v3 corpus contains 71,389 positions, including 9,317 usable
  deeper-search labels, with grouped train/validation/test splits.
- The live label collector had completed 11,808 searches from 617 scanned games.
  Its selected-position backlog was empty before it paused for training.
- The frozen teacher was `NNUE_W512_v2`, with 10 seconds per selected position.
  About 79% of those searches completed depth 3 or 4; median wall time was about
  10.75 seconds including overhead.
- Sampling selects at most approximately 20 positions per eligible game: 12
  representative progress strata, up to five large evaluation swings, and up to
  three positions near decisive endings. Overlaps and eligibility reduce counts.
- The collector excludes games using historical engine bindings. Its empty queue
  therefore does not mean all recorded games or all useful positions were labeled.

These observations support investigating both coverage and search quality. They
do not establish that labeling speed exceeds game generation under every future
field, board distribution, or budget. When analysis drains its queue, the current
coordinator returns the shared CPU to games; extra labeling has an opportunity cost.

## 2. Why probability-based targets are useful

The current probability-target recipe compares sigmoid-transformed total scores:

```text
prediction = sigmoid((material + learned_correction) / scale)
target     = sigmoid(teacher_score / scale)
loss       = weighted squared difference
```

The scale is calibrated on training data, not the held-out test set. This is a
scalar expected-outcome-style target, not a separately modeled win/draw/loss
triple. Its calibration is approximate; the current fit excludes ambiguous draw
outcomes because recorded draws do not reliably distinguish game caps from rule draws.

An error near equality can change which side appears favored. A change from
+10,000 to +20,000 deserves much less emphasis **if both scores reliably imply
nearly certain victory**. The transformation reflects that distinction. If large
advantages are less reliable in our game, calibration should compress them less.

A hidden mating threat means an evaluation target may be wrong; preserving the
raw numerical difference between two large positive scores does not fix that.
The transformation changes the training loss, not search's score representation.

For context, the first completed 256-v3 model reduced held-out validation mean
absolute error from about 1,822 units for material alone to 1,162 units, and the
probability objective by about 81%. Those percentages measure teacher prediction,
not Elo or winning percentage. Material-only is also a much weaker comparator
than the existing complete handcrafted or NNUE engines.

## 3. Ways to use more labeling compute

### Broader representative coverage

Sample more well-spaced positions from the games already available. A long game
contains many potentially different situations beyond the current 12 general
samples. Favor additional coverage over long blocks of adjacent, similar plies.
Cap each game's contribution so very long games do not dominate training.

Maintain a substantial representative/random component even when adding targeted
samples. Otherwise the model may become good at the analyzer's selection criteria
while losing accuracy on ordinary positions encountered in play.

### Selectively longer searches

Compare the current 10-second labels with 30–60-second labels. More search can
reveal an attack or defense that the network can subsequently learn to recognize
without repeating that search. More time is not a guarantee of a better label,
and nominal depth is not comparable across all positions or selective policies.

Useful prioritization signals include:

- Material changes in assessment between completed search iterations.
- Disagreement between strong teacher engines.
- Disagreement between static evaluation and searched evaluation.
- Positions preceding a reversal or royal loss.

Use probability-space disagreement where appropriate: a change from roughly equal
to probably losing generally matters more than a change between two overwhelming
advantages. These signals identify candidates for investigation, not known errors.
Stable scores can still be wrong; unstable scores do not prove extra search will help.

Retain the deepest completed iteration on interruption, alongside completed depth,
node count, elapsed time, teacher identity, search policy, and timeout status.
Do not treat an unfinished iteration or an alpha/beta bound as an exact label.

### Better teachers and disagreement checks

As tournament evidence identifies stronger models, test them as label teachers.
A stronger teacher may be more useful than giving the old teacher increasingly
long searches. The widest network is not automatically the best teacher at a
fixed CPU budget.

Keep labels versioned by model contents and search policy. Use disagreement to
request verification rather than blindly averaging incompatible scores or mixing
old and new labels without provenance. Compare teachers under matched compute.

### Alternative continuations without full games

A tournament records only the moves actually played. From selected positions,
try another plausible legal move, continue briefly, and label the resulting
positions with search. This can add examples of defenses, failed attacks, and
plausible mistakes at much lower cost than completing another long game.

Score-target training does not require a final game outcome for these samples.
Keep outcome fields unknown rather than inventing results for unfinished branches.
Use controlled alternatives, such as competitive root candidates, rather than
arbitrary illegal or overwhelmingly bad moves. Preserve the full prefix, side to
move, repetition history, and draw counters when branching.

Keep every branch with its source game in the same dataset split. Diversity from
one parent position is not the same as independent games.

## 4. Mate-score positions: a concrete missing category

The current pipeline deliberately excludes scores with absolute value at least
900,000. The default mate score is 1,000,000. Relevant exclusion points include:

- [Live label sampling](../../deploy/training_labels.py).
- [Pilot sampling and deeper-label import](pilot_data.py).
- [Legacy starter preparation](prepare.py).
- [Rust dataset export](../../src/bin/nnue_tool.rs).

Changing only one filter will not make these positions reach training.
Use explicit label types in a future dataset version rather than assuming every
large integer, across all historical engines, has identical mate semantics.

Three categories need different treatment:

1. **Already terminal:** the last royal is gone, or a game rule establishes the
   result. Search/evaluation handles this explicitly before calling the network.
   Such examples offer little new information to the current evaluator.
2. **Nonterminal with a verified forced win/loss:** a valuable label for a position
   whose tactical outcome a shorter search may fail to discover.
3. **Earlier attack/defense positions:** often more relevant to move choice, but
   each needs its own assessment. A later mating finish does not establish that
   the earlier position was already lost.

The existing engine also emits mate scores for last-royal check positions with
no saving evasion. Its rules permit play to continue until the royal is actually
taken, so a mate-scored game position need not already be terminal. Treat the
actual game rules, including multiple royals, as authoritative rather than
importing chess assumptions.

### Targets: categorical outcomes instead of huge regressions

For a verified nonterminal mate, use a target of 1 for a side-to-move win or 0 for
a side-to-move loss in the existing probability loss. Ordinary finite-score
positions keep their calibrated soft targets. Lower-confidence search claims
must remain distinguishable from verified labels; weighting or softening them
would be an experiment, not a claim that they are proven.

Do not regress toward +1,000,000 or -1,000,000, and do not obtain mate targets by
passing sentinel scores through the ordinary residual-clipping path. An explicit
`label_kind` and probability target avoid conflating a categorical outcome with
an arbitrary engine score convention.

The NNUE continues to emit an ordinary finite evaluation. Search retains special
mate scores for outcomes it establishes. No auxiliary mate head or inference
architecture change is necessary for this first experiment. Do not infer mate
length from our fixed mate-score sentinel.

For a material-rich but losing position, the correction should outweigh material.
Check this explicitly: the inference residual is currently bounded to ±100,000,
and the combined nonterminal evaluation is bounded below the mate-score range.
Audit whether any selected positions require corrections outside those bounds,
and keep training and deployed clipping behavior aligned. A bounded target
alone does not fix output-capacity or saturation problems.

### Verification: a reported mate is not automatically a proof

Our search uses pruning, reductions, and selective candidate handling. Re-search
candidate mate positions with more time and a conservative verification policy.
Record which shortcuts were disabled, what defensive alternatives were covered,
and whether the search completed.

Replaying a legal winning principal variation verifies that line, not that all
opponent defenses lose. Reserve a "verified forced outcome" label for the level
of coverage actually established. A longer ordinary search can increase confidence
without becoming a proof. Do not silently promote a timed-out claim to certainty.

Verification must preserve repetition/draw history and use the correct scoring
perspective. Tests should cover a winning and losing side to move, multiple
royals, an available saving defense, and an apparent mating line invalidated by
a rule or a missed reply.

### Examples that teach the boundary between danger and loss

Collect a small number of contrasting positions per sequence:

- The earliest examined position where verification establishes the forced result.
- Earlier positions, each labeled by its own search.
- A position where a defense prevents the loss.
- The successor after a plausible mistake makes the attack decisive.

The "earliest examined" point is a search finding, not necessarily the true first
forced win in the game. Do not propagate a later win/loss label backward through
moves that may have changed the outcome. Include both attacking and defending
examples, and retain history for counterfactual branches.

Start experiments with approximately 5% and 10% mate-related samples by effective
training draws or weight, explicitly recording which interpretation is used.
These are trial proportions. Cap per-game contributions and deduplicate similar
sequences. The combined category should contain defensive contrasts as well as
forced outcomes, not merely many copies of an inevitable finish.

## 5. Experiments to decide what is worth scaling

### A. Coverage versus deeper labels at equal labeling cost

A nominal 9,000 CPU-second budget could buy:

- 900 additional positions at 10 seconds each.
- 300 positions at 30 seconds each.
- A mixture of broad coverage and selectively deeper labels.

Account for replay/setup overhead and actual elapsed CPU time. Reusing an existing
10-second result is not necessarily a resumable search: a 30-second re-search
may cost the full 30 seconds. Keep base corpus, checkpoint, model width, effective
training budget, and search settings fixed. Use multiple training seeds when
practical; one seed cannot characterize training variance.

### B. Mate examples on the existing 512 recipe

Compare the current recipe with 5% and 10% verified mate-related samples plus
nearby defensive contrasts. Keep the same parent and architecture. Match total
training draws so an apparent gain is not just additional optimization time.
Distinguish replacement sampling from adding data on top of the baseline.

Hold out whole games and related branches. If targeted samples replace normal
samples, report the resulting ordinary-position coverage change. Do not allow
newly collected positions from existing held-out games into training.

### C. Measure the intended behavior, then playing strength

Report general validation metrics and a separate tactical set containing attacks,
working defenses, and superficially dangerous but safe positions. Useful outcomes
include the chosen move, loss avoidance, attacking move discovery, and nodes/time
needed at a fixed game budget. Count false alarms as well as found attacks.

A deeper teacher's agreement with itself is not independent ground truth. Use
verified outcomes where available, and disclose uncertainty elsewhere. Finish
paired games from more independent starts than the original four-start pilot.
Game caps remain unresolved rather than draws. Do not select models repeatedly
against the same final test games.

Our small pilot favored continued training with probability targets as a next
candidate, but most continuation games hit the cap. It did not settle whether
outcome mixing, deeper labels, mate examples, or a particular width is strongest.

## 6. Suggested order and operational boundaries

1. Audit and count nonterminal mate claims already present in saved games/labels.
2. Implement explicit label kinds, full-pipeline export support, and verification.
3. Run the controlled 512 mate-data experiment with defensive contrasts.
4. Compare broader sampling with adaptive deeper searches under matched compute.
5. Add alternative continuations and evaluate stronger teachers as evidence warrants.

This order is a research priority, not authorization to change the live service.
Keep the current generation's dataset frozen. New collection policies, teacher
versions, budgets, and target semantics get new identities and resumable outputs.
Preserve completed search results immediately; interruptions must not corrupt or
misidentify labels. Use the existing shared-CPU mechanism and watchdogs, with
visible status and game-worker fallback when the analyzer has no useful work.

Do not interpret this document as evidence that these changes improve strength.
The next experiment should isolate one decision well enough to justify scaling it.

## 7. External references and limits of transfer

- [Stockfish NNUE training explanation](https://github.com/official-stockfish/nnue-pytorch/blob/master/docs/nnue.md?plain=1)
  explains sigmoid score targets, mixing game results, and probability-space
  losses. It supports the target formulation; it does not establish the best
  calibration, architecture, or mate-sample proportion for ultimate shogi.
- [Fairy-Stockfish data-generation guide](https://github.com/fairy-stockfish/variant-nnue-pytorch/wiki/Training-data-generation)
  exposes data volume/search-depth choices and controlled randomized move
  selection. Its example counts and depths are not prescriptions for a 36×36
  board with our branching factor and compute budget.
- [Stockfish's older training guide](https://github.com/official-stockfish/nnue-pytorch/wiki/Basic-training-procedure-%28train.py%29)
  discusses tactical filtering and training variability.
  [A later successful network update, PR #5254](https://github.com/official-stockfish/Stockfish/pull/5254),
  explicitly used data without best-move captures removed, alongside several
  other changes. This is evidence that filtering practice varies, not an isolated
  proof that retaining every tactical position helps.

Our sparse human expertise and very large board motivate automated discovery,
label verification, and careful sampling. They do not establish that every
technique effective in chess or standard shogi will transfer unchanged.
