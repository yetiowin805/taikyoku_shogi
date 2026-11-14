# Future notes

Ideas parked after a knockout, not the next patch. Search / eval experiments that are ready to measure still live in [`IDEAS_TO_TRY.md`](IDEAS_TO_TRY.md).

## Recurring game review: conversion and tactical awareness (2026-11-13)

Revisit these questions after a meaningful training/search generation change and
periodically as fresh tournament games accumulate. Inspection of v6_search games
suggests three useful research directions; their prevalence and causes have not
been established by a representative audit. These are proposed investigations,
not implemented improvements or a reason to change the running training recipe.

Start with a small fresh sample as well as the extreme examples below. Record
which failure is reproducible, the relevant position/history, plausible defenses,
and whether additional search resolves it. Pick one bounded training or search
experiment at a time and keep ordinary-play controls.

### Converting an advantage against the last royal

Some very long draws appear to involve an agent that cannot finish against the
opponent's last royal despite an apparent advantage. Distinguish a genuinely
unconvertible position or rule draw from a missed forced finish, repeated checking,
poor coordination, or failure to make progress before the game cap. A large
recorded evaluation alone does not establish a forced win.

- **Inspect:** replay the final-royal phase, identify when conversion stalled, and
  check whether stronger/better-directed search can demonstrate a legal finish.
  Include escapes, defensive resources, promotions creating another royal, and
  the actual repetition/progress/clock state. Follow this variant's terminal and
  legal-move rules rather than assuming orthodox checkmate rules apply.
- **Training experiments:** collect verified finishing sequences, earlier
  conversion decisions, and hard defenses from these endgames. Balance them with
  positions where chasing the royal fails; cap each game's contribution. Consider
  a conversion/progress auxiliary target only when its labels can be justified.
- **Search/eval experiments:** investigate bounded mate/royal-capture verification,
  better forcing-move ordering, attacker coordination and escape-square control,
  or a progress-sensitive conversion term. Check that any selective mate search
  is sound under the variant's legal defenses. Avoid rewarding checks or proximity
  that do not actually improve the position.
- **Measure:** conversion rate on a held-out endgame suite, plies/time to finish,
  avoidable repetitions and game-cap draws, plus false attacks, lost wins and
  search-budget cost. Do not count a shorter loss or an unsound attack as progress.

### Tactical and trapped-piece awareness

Investigate pieces that are materially valuable but lack safe mobility, are
trapped after a natural move, or can be won by a tactical sequence that shallow
search/evaluation misses. Distinguish an actual trap from a temporarily restricted
piece with a safe exchange, escape, promotion, or counterattack.

- **Inspect:** use sustained evaluation-drop episodes to find the earlier decision
  and analyze legal alternatives on the same board. Classify immediate hanging
  pieces, traps requiring preparation, discovered/path captures, and bad exchanges.
  A later score collapse is a candidate-finding signal, not proof of a blunder.
- **Training experiments:** add positive and negative trap examples, escape moves,
  and plausible but losing alternatives from search. Test whether existing inputs
  can represent the relevant mobility/attack context before adding features.
- **Search/eval experiments:** audit capture/threat eligibility, pruning and
  quiescence at the missed tactic; test safe-mobility or tactical-exposure features
  and selective extensions only where justified. Respect multi-leg and path-capture
  semantics, including changes to occupancy after intermediate captures.
- **Measure:** held-out trap/escape decision quality, false alarms against sound
  sacrifices or temporary confinement, same-budget playing strength, and node/time
  overhead. A better teacher fit alone does not establish better play.

### Unexpected mating attacks, especially with jumping pieces

Review attacks where an agent remains confident until its last royal becomes
unsavable, including jumps over apparently protective pieces and combinations
with range or multi-leg attackers. Check threat recognition and defensive move
selection before the terminal score appears, not just the final capture.

- **Inspect:** reconstruct the attack and its best legal defenses; distinguish
  missed attack-map semantics, a horizon/pruning miss, a bad evaluation, and a
  misleading incomplete search result. Include similar-looking attacks that
  cannot be forced.
- **Training experiments:** mine attack precursors and defensive alternatives,
  with emphasis on jumping routes, coordination, and misleading shelter. Verify
  labels with stronger analysis or a sound terminal proof where available; do not
  promote the agent's own confident mate score to ground truth automatically.
- **Search/eval experiments:** audit legal threat/capture generation and royal
  safety inputs for jumping attacks. Test targeted threat/evasion ordering or
  extensions, and whether forcing quiet moves need selective quiescence coverage.
  Neighboring flight squares alone need not capture longer-range or jumping threats.
- **Measure:** attack/defense success at the tournament time budget, how early the
  threat is recognized, false mate/threat alarms, and regressions from expanding
  too many forcing moves. Keep multi-royal positions as controls.

### Evidence to revisit and how to select fresh examples

The November 13 inspection used the `corrected-rules-20261031` VPS tournament.
Curated games and their `INDEX.txt` / `selection.json` are local, gitignored data;
they are not included in this PR. The run and slot IDs identify source games if
these local collections are no longer available.

- `data/raw/tourney/v6-search-longest-draws-20261113/`: slots **5359, 5284, 4296,
  4587, 5100**, selected by recorded ply count among completed draws involving
  v6_search. These are extreme cases, not a draw-cause frequency estimate.
- `data/raw/tourney/v6-search-durable-expected-drops-20261113/`: slots **5658, 4395,
  5072, 4445, 3992, 5482, 5307, 5065, 4161, 5025**. These are investigation seeds;
  review is needed before assigning any of them to a failure category above.

For fresh sustained-drop candidates, compare the **same agent's** recorded search
scores, converted to its perspective (negate Black-absolute scores for White).
With `E` denoting an expected-final-score mapping, rank positive values of:

```text
min(E(x-6), E(x-4), E(x-2), E(x))
  - max(E(x+2), E(x+4), E(x+6), E(x+8))
```

Require all eight samples to exist and, for the early-game review, all to lie in
the first half of the game. Samples cover 14 plies; every earlier sample must
exceed every later sample. Group overlapping windows from the same episode when
sampling for review or training so a single event does not dominate.

The exploratory mapping used `E(s) = sigmoid((s - 1714.6) / 2991.6)`, fitted to
v6_search-versus-v6_search outcomes with draws worth 0.5. It is an approximate
ranking aid for that snapshot, not a universal evaluator or a proposed training
target. Recheck calibration after model/search changes; distinguish mate-band
scores and opponent/phase/family effects. Preserve raw scores as well as mapped
values. Raw zero need not imply a 50% eventual score.

**GUI ply `n`** labels the record at `moves[n-1]`; replay `n-1` to analyze the board
where that move's search began. Preserve the full replay state/history and engine,
model and clock provenance. Do not compare neighboring agents' evaluations as if
they were independent assessments of one identical position.

### Experiment and follow-up discipline

- Refresh a small representative sample along with the worst cases; record
  counterexamples and unresolved cases. Keep game/start families together when
  splitting development examples and held-out checks. Separate targeted gains
  from ordinary-position regressions.
- Compare a frozen baseline and one candidate under matched starts/colors and
  clocks. Record revisions, model hashes, sample sizes, uncertainty, correctness
  checks and resource cost. Run labeling, builds and substantial suites on the
  authorized VPS/CI, within its available budget.
- Improve labels or sampling when a stronger teacher reliably recognizes the
  mistake; investigate search first when the necessary candidate/defense is never
  examined. If both fail, retain the uncertainty rather than training on an
  unverified correction. Verify the eventual playing result either way.
- After an experiment, add its result and next question here; promote a concrete
  proposal into [Ideas to try](IDEAS_TO_TRY.md). Follow the existing history-freeze
  rule for a future major eval/search behavior change; this research note itself
  does not change engine behavior.

Related: [last-royal flight and tropism ideas](IDEAS_TO_TRY.md#l--last-royal-flight-penalty),
[NNUE data/mate roadmap](training/nnue/DATA_ROADMAP.md), and
[v6 collection and training research](training/nnue/V6_RESEARCH.md).

## Search performance follow-ups (2026-09-12)

The current speed change implements **cached ordering keys** and **skipping
disabled sibling bookkeeping**. Keep the other proposals separate so we can
measure their marginal benefit after these two changes.

The initial experiments used main `1d8830d`, 18 opening/tournament positions,
11 C2/S2/L/A checkpoints, and one pinned i7-1255U laptop CPU. Three shuffled
depth-2 repetitions measured about **10.3%** less elapsed time for cached keys
and **4.2%** for skipping dormant sibling work, relative to an all-flags-off
experimental control. Fixed-depth scores, chosen moves, returned root lines
and node counts matched. Faster execution can still change timed-search
results by completing more work; these figures do not establish playing
strength or VPS throughput.

The earlier five-prototype combination saved 17.8% across equally weighted
positions (13.3% by total time). That result includes three additional changes
and must not be quoted as the measured gain of these two changes alone.

The clean two-change build was also compared directly with `1d8830d`, without
experimental flags: **17.0%** less time at depth 2 across equally weighted
positions (**12.3%** by total time), and **29.6%** on the selected six-position
depth-3 subset. Both used three paired repetitions. All 168 fixed-depth
invocations matched in scores, chosen moves, returned root lines and node
counts, including extra depth-3 checks of sibling modes 1–4 on three positions.
The deeper subset was selected for affordable depth-3 searches, so it is not
an unbiased estimate for all positions.

### Tested without a clear overall benefit

- **Indexed root scores:** 0.0% aggregate depth-2 saving; a few wide roots
  improved by several percent. Revisit only with a stronger root-reordering
  cost measurement. Preserve stable ties and the existing first-match
  from/to/promotion identity, which ignores intermediate routes.
- **Count-only mobility:** 0.3% aggregate saving, below the earlier 1–8%
  hypothesis. The prototype removed Range landing-target vectors, not all
  allocation or repeated evaluation. Incremental mobility is a separate,
  harder experiment: every ray affected by intermediate captures must be
  invalidated.
- **Quiet diagnostics:** -0.8% aggregate saving, within run variation. The
  prototype suppressed eager move labels and progress messages. Lazy
  formatting that retains the messages, and throttled deadline checks,
  remain untested. Any clock experiment must measure timeout overshoot.

### Unmeasured candidates

The ranges below are the original engineering hypotheses for whole-search
elapsed-time reduction, **not measured gains or confidence intervals**. Zero
gain or regression remains possible, and gains overlap. Start with tactical
discovery reuse before changing pruning or search order.

- **Reuse leaf-gate / qsearch tactical discovery (3–12%):** share promotions,
  royal takes and eligible hanging captures for the same position and policy.
  Preserve candidate order and mandatory last-royal evasions.
- **Constant-time board piece-list updates (3–10%):** index squares to slots.
  A naive swap-remove changes traversal order, floating-point accumulation
  and potentially move choice; validate complete make/unmake restoration.
- **Reusable per-ply move/undo buffers (2–8%):** reduce allocation without
  changing candidate order, route ownership or capture capacity.
- **Incremental repetition counts (0–8%, especially long games):** retain the
  entire inherited history and current search/game repetition rules, including
  null-move push/pop. A map may lose to a short vector scan in openings.
- **Borrowed virtual-board attacker iteration (1–6%):** avoid cloning armies
  for simulated capture checks. Validate every removal, promotion and route;
  this overlaps with the ordering-key improvement.
- **Reuse TT storage (0–2% at a 3-second budget):** reuse capacity while keeping
  each search logically empty. Retaining entries across turns is a separate
  behavior-changing experiment.
- **TT-first staged generation (5–20% when effective):** avoid full generation
  before an immediate cutoff. This changes staging/order and interacts with
  LMR, PV-dependent qsearch and route identity; test playing strength too.
- **Interior PVS / aspiration windows (5–25% when effective):** test separately.
  PV-dependent q-depth can change the selective tree; volatile scores and
  repeated re-searches can make either approach slower.
- **Target-directed multi-leg capture generation (3–15% on relevant boards):**
  avoid enumerating routes that miss the target. Require the complete ordered
  candidate sequence to match before treating it as behavior-preserving.
- **Analyzer replay/process batching (0–5% for typical 30-second searches):**
  share immutable game parsing and replay prefixes. This affects catalogue
  throughput, not tournament search; preserve worker isolation, model/cache
  identity, durable iteration results and watchdogs.

For later experiments, reuse varied opening, promotion-heavy, long-history
and last-royal positions with the original checkpoints. Compare fixed-depth
results first, then repeat paired timing at fixed depth and 3/30-second
budgets. Report completed depth and score/move changes alongside node counts.
The local corpus was purposefully varied, not weighted to live-game frequency.

## Top-11 Texel twins (22-agent knockout)

**When:** after looking at hang-q A/B games, and after A+B is default search on `main`.

**Field:** the mix-tournament top 11 (not leftover-only history), each with a Texel twin → 22 agents, play-in bracket.

| # | Chassis |
|---|---|
| 1 | `T150_P120_T12` |
| 2 | `H120_P120_T15` |
| 3 | `AVG_T150_H120` |
| 4 | `C2K50A1` |
| 5 | `BASE_P120H50B75` (`H105_P120_T15`) |
| 6 | `BASE_H120O80` (`H120_OLD_T15`) |
| 7 | `SEED` |
| 8 | `H120_B65_T12` |
| 9 | `AVG_P120_SEED` |
| 10 | `T150_B65_T12` |
| 11 | `C2K100A1D50` |

**Inspect first (11 fits):** `./deploy/run_top11_texel_fits.sh --games-dir data/raw/tourney/top4-mix-swiss-…` (add `--skip-featurize` if `data/derived/top11-texel` already exists). Each chassis `--init`, same features. **Only range two-movers + range capturers move**; Golds / pawns / royals stay on the parent. `/Pawn` %Δ in `models/top11-texel/compare.md`. An unconstrained all-piece fit collapsed Hook/VG to ~0 and exploded the mid table — do not use that.

**Later transplant (one shared table, not 11):** if the 11 fits agree on the loud pieces, take one of those tables (or a seed-init fit on the same features) and copy **large pieces only** onto each chassis. Current `texel-fit` only trains the **piece-value** vector (`piece_diff`). PST, tropism, and `two_mover_mob_k` stay at `--init`.

**Transplant:** copy each chassis (PST / trop / average / C2 extras) and replace **large pieces only** (Hook, Cap, Tengu, Peacock, GG, other two-movers / capturers). Leave small pieces on the parent so T150 vs H120 twins do not collapse to one mid table.

**C2K50A1 twin extra:** also move `k`. Rows have no mobility feature, so either add a two-mover-mobility column and fit `k` with the pieces, or keep the piece transplant and **line-search `k`** on the same CE. Prefer the line-search unless `k` is going into Texel for good.

Do **not** bake onto `models/ab-seed.json`. Both the control 11 and the twins should run with dest hang-q on (A+B default).
