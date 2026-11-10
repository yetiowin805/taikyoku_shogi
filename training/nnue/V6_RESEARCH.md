# NNUE v6 data collection and training proposal

Research date: November 3, 2026. Repository inspected at `0471aec`.

This proposal assumes that **all approximately 1,600 new games were played under the corrected rules**, as confirmed in the discussion. They are the primary v6 source. The range two-mover issue concerns knowledge inherited by the evaluators, not the legality of these new games. The proposed starting networks are 384v4.5 and 512v3. Their apparent strength, and v5's apparent regression, are working observations rather than newly measured strength estimates.

The recommendation is to keep the full game archive, build a smaller, broadly sampled and freshly analyzed training core, add a modest number of alternative continuations, and use exactly that frozen collection for four training regimes at both widths. Use game results cautiously as an independent corrective signal. Do not decide which teacher is right from the identity of the eventual winner.

The four regimes are:

1. **A — Fixed-material control:** the existing residual representation with a conventional, outcome-mixed cross-entropy objective.
2. **B — Pure NNUE:** a network that predicts the total score, using the same objective and data as A. This is the most important direct comparison.
3. **C — Pure NNUE with robust score correction:** confidence-weighted regression in normalized score space, designed to correct confidently wrong evaluations without chasing extreme teacher scores.
4. **D — Pure NNUE with alternative-move ranking:** B's value training plus a small objective comparing freshly analyzed sibling positions. This is the experiment with the most potential to improve decisions beyond fitting scores.

This incorporates the proposal to retire fixed material from new models. My recommendation is to test that transition directly in A versus B, then adopt it if playing results support it. Existing residual-model compatibility should remain; there is no need to reinterpret old binaries as total-score networks.

Percentages, sample counts, learning rates, and time budgets below are proposed initial settings. They are not measured optima. This report does not establish why v5 was weaker or predict an Elo gain.

## What other engines actually suggest

### Stockfish

Stockfish's dataset documentation describes randomized self-play, varied openings, and large collections of positions. Its historical generation example uses depth 9 and bounded MultiPV randomization; the same page also discusses Lc0-derived data and explicitly notes that stronger or deeper labels do not necessarily produce stronger trained networks. The useful lesson is to balance label quality, diversity, and what a small static evaluator can learn. The documented billions of positions are context, not a minimum we should attempt to reproduce. [Stockfish training datasets](https://github.com/official-stockfish/nnue-pytorch/wiki/Training-datasets).

The inspected trainer supports interpolation between search scores and outcomes, probability-space power loss, and quantization-aware training. Its configuration defaults to a power of 2.5; the older explanatory documentation gives 2.6 as an example that had worked. Neither number is a universal NNUE prescription. Also, the inspected default teacher mixture is 1.0, so supporting outcome mixing does not mean every Stockfish training run uses it. [Loss implementation](https://github.com/official-stockfish/nnue-pytorch/blob/c625702757b58999611b006b5a03e3e1c5e142ea/model/nnue.py), [configuration](https://github.com/official-stockfish/nnue-pytorch/blob/c625702757b58999611b006b5a03e3e1c5e142ea/model/config.py), [NNUE explanation](https://official-stockfish.github.io/docs/nnue-pytorch-wiki/docs/nnue.html#loss-functions-and-how-to-apply-them).

Its loader can skip captures, checks, early positions, and random positions. These are configurable mechanisms, not proof that every production dataset applies every filter. For us, this supports inspecting tactical noise and repeated opening material; it does not justify deleting all royal-danger positions. [Stockfish loader](https://github.com/official-stockfish/nnue-pytorch/blob/c625702757b58999611b006b5a03e3e1c5e142ea/data_loader/cpp/training_data_loader.cpp).

### YaneuraOu and shogi training

YaneuraOu's learning documentation separates position generation from evaluation generation and training. Its current overview describes re-evaluating generated positions with a different, potentially larger teacher, including distillation from deep-learning shogi evaluators into NNUE. This is directly relevant: the engine that played a game need not supply its final training targets. It also means evaluator diversity can matter more than using several descendants of one network. [YaneuraOu learning overview](https://github.com/yaneurao/YaneuraOu/wiki/%E8%A9%95%E4%BE%A1%E9%96%A2%E6%95%B0%E3%81%AE%E5%AD%A6%E7%BF%92).

The historical `gensfen` documentation provides early-position filtering, random moves, and bounded MultiPV alternatives. Its exact ply limits and score units are specific to ordinary shogi. On a 36×36 board, phase and structural change are more useful than copying its opening cutoff. [Archived learning-command documentation](https://github.com/yaneurao/YaneuraOu/wiki/%E3%82%84%E3%81%AD%E3%81%86%E3%82%89%E7%8E%8B%E3%81%AE%E5%AD%A6%E7%BF%92%E3%82%B3%E3%83%9E%E3%83%B3%E3%83%89/58956444170f23f749bca9f007f50763db8b6b09).

The historical Elmo-style learner combines teacher probabilities and actual game results in a cross-entropy gradient. That is a useful precedent for regime B, though the cited 2017 implementation predates the NNUE architecture and is not a statement about every modern YaneuraOu recipe. Be careful with parameter names: in that implementation lambda weights the teacher; some other trainers use a blend parameter that weights the outcome instead. [Pinned historical learner](https://github.com/yaneurao/YaneuraOu/blob/b6e096cd323ae59d3089996cacb3095271c2abbe/source/learn/learner.cpp).

### Fairy-Stockfish, Viridithas, and Bullet

Fairy-Stockfish's generation guide explicitly exposes the difference between quiescence and static evaluation as a quiet-position filter, as well as bounded alternative moves and existing-network bootstrapping. This is particularly useful for interpreting the proposed raw-versus-searched discrepancy signal. Its chess-variant depths and recommended dataset sizes should not be imported directly into Taikyoku. [Fairy-Stockfish generation guide](https://github.com/fairy-stockfish/variant-nnue-pytorch/wiki/Training-data-generation).

Viridithas provides a concrete example of making diverse starts, generating its own games, and rescoring data. The inspected generator uses randomized initial moves, a static-exchange heuristic when proposing them, and node-limited play. Its exact chess constants are not transferable, but dedicated training-data generation is: tournament games need not be our only source of positions. [Project training history](https://github.com/cosmobobak/viridithas), [pinned generator](https://github.com/cosmobobak/viridithas/blob/7489e701bb07ff9a7223136bac7393f417aa588e/src/datagen.rs).

Bullet's example demonstrates sigmoid squared error, outcome/search blending, and a scheduled learning rate. Its documentation warns that imitating a sophisticated Stockfish architecture without sufficient data and implementation work can lose strength. That favors keeping the 384/512 backbone while testing the output representation and targets. Its example batch size of 16,384 is not a recommendation for our much denser sparse-feature representation. [Bullet example](https://github.com/jw1912/bullet/blob/da426a49b2a4b79a55a87031cd51c89bdf1ff030/examples/simple.rs), [Bullet development guidance](https://github.com/jw1912/bullet/blob/da426a49b2a4b79a55a87031cd51c89bdf1ff030/docs/1-basics.md).

There is also research precedent for training on positions inside a search tree, rather than only played positions. Veness and colleagues distinguish root and tree bootstrapping, explain the mismatch between game positions and search positions, and treat search bounds differently from exact values. Regime D borrows the motivation, not their complete algorithm or any claimed performance transfer. [Bootstrapping from Game Tree Search](https://papers.nips.cc/paper_files/paper/2009/file/389bc7bb1e1c2a5e7e147703232a88f6-Paper.pdf).

## What the previous generations tell us

The supplied history is consistent with the local training documentation: v3 and v4 used relatively sparse samples, v4.5 and v5 expanded to dense recent-game positions, and v5 compared three fairly similar mate/precursor mixtures from 512v4. Importantly, v4.5 continued from v3, whereas v5 continued from v4. That is a major confound when interpreting their relative strength. The generation sizes alone do not isolate a sampling effect.

The [v5 recipe](V5.md) already balances stratum, related-game group, game, phase/episode, and position. It caps the parent-error multiplier and keeps parent exposure out of holdouts. Therefore, it would be inaccurate to diagnose v5 simply as letting long games dominate. Dense storage can be useful even when only a small, carefully weighted fraction is drawn.

The most plausible hypotheses to investigate are:

- **Teacher bias:** saved labels and cached 512v4 analysis could reinforce a common evaluation error. Averaging correlated teachers does not create independent evidence.
- **Starting-point quality:** v5 inherited a different parent from the stronger-looking v4.5 lineage.
- **Objective mismatch:** matching a teacher more closely need not improve move choice under a shallow, selective search.
- **Correlated evidence:** a million rows may still describe relatively few starts, trajectories, and decisive episodes.
- **Difficult-label emphasis:** parent error identifies disagreement with the target, not necessarily a parent mistake. It can emphasize bad labels or tactical positions a static net cannot represent reliably.
- **Limited decision coverage:** played trajectories omit many plausible defenses and tempting mistakes encountered during search.
- **Training exposure:** fixed draw windows do not imply complete dataset coverage. Conversely, indefinite continuation on the same labels does not add information.

These are hypotheses, not a causal ranking established by experiments. The [cached error audit](../../benchmarks/training_weights/README.md) itself explicitly treats parent error as teacher discrepancy. The [sampling audit](../../benchmarks/sampling/README.md) also notes that adjacent-player scores describe neighboring boards.

Before a v6 launch, record each parent's and v5's quantized prediction errors, saturation rates, and same-search playing results under the corrected rules. Use this to understand the starting point, not to retroactively tune a final test set.

## How to use the proposed disagreement and winner signals

### Compare the same position, with the same score orientation

Two consecutive saved evaluations generally refer to different boards. Their difference combines evaluator disagreement, the effect of the played move, search noise, and potentially unequal clock budgets. Interpolating an opponent's neighboring evaluations is a useful cheap candidate-finding heuristic, but cannot establish same-position disagreement.

At selected boards, evaluate both frozen teachers directly. Store Black-absolute scores as evidence and convert explicitly to side-to-move targets for training. Preserve full state and history for analysis. A bare board is not enough to reconstruct repetition, promotion provenance, or progress counters.

Two agents with the same model content are one evaluator family, even if their names differ. Different search settings can produce useful search-policy disagreement, but should not be counted as independent model votes. A handcrafted agent is a separate evaluator, not automatically equivalent to the NNUE agent it played against; both may still share material values and search assumptions.

### Decompose raw-to-search disagreement

For a common probability mapping `P(score)`, record:

```
tactical_gap  = |P(static score) - P(quiescence score)|
strategic_gap = |P(quiescence score) - P(deeper search score)|
teacher_gap  = |P(teacher A search) - P(teacher B search)|
budget_drift = |P(search at budget B) - P(search at larger budget)|
```

Large tactical gaps often indicate pending captures or forced sequences. These can be important, but blindly maximizing them spends labels on situations search already resolves. Larger strategic gaps can identify patterns that a better static evaluation might make accessible at shallower depth. Neither gap proves an error. Our quiescence is selective, so its output is not a universal tactical truth.

Use these quantities to allocate analysis, with random and representative coverage retained. After a position has a good label, its training weight need not be proportional to the gap that caused us to inspect it. Keep raw-score gaps as diagnostics too: a probability transform can hide relevant differences between saturated evaluations.

### Use the winner as evidence about the outcome, not about the teacher

I would not adopt “prefer the eventual winner's evaluation after the last zero crossing.” That condition uses the future trajectory to select evidence, depends on score calibration, and is common in already settled games. A winner can be wrong for many moves, then benefit from a later blunder or a time forfeit. A loser may have evaluated an earlier advantage correctly.

Instead:

- Use decisive results to identify cases where confident evaluations failed, on both players' turns.
- Search the plausible alternative defenses around a reversal. This helps distinguish a bad evaluation from a later bad move.
- Include a small outcome component for games with reliable, relevant endings.
- Treat “both evaluators remain confident but reality differs” as a high-priority audit category; consensus alone misses shared errors.
- Retain successful defenses, recoveries, and wins with fewer range two-movers, as well as losses involving them.

Even reliable outcomes are samples of the players' policies, not perfect-play values. A small result mixture can correct systematic bias; a large mixture on 1,600 games can teach the network to predict player strength or later blunders. “No more zero crossings” can be logged for descriptive analysis, but should not determine label ownership or outcome weight.

## One shared data collection plan

### Preserve dense records, control the training distribution

Keep every usable record in an immutable archive. There is no advantage in throwing away information just because it will not receive equal training weight. Produce three separate objects:

1. A position index with full provenance and links to game histories.
2. A versioned label catalogue containing individual teacher observations.
3. A frozen training manifest specifying eligibility, target construction, splits, and sampling probabilities.

This separation lets a later generation use more of the archive without pretending the original saved labels improved. Retain individual scores rather than only an averaged target.

### Split before selecting targeted examples

Build one split manifest shared by all eight width/regime combinations. Group paired starts, explicit start families, substantial shared nontrivial prefixes, duplicates, and every generated descendant. Do not merge every standard-opening game merely because it shares the initial board. Check inherited exposure against the **union of both parents' training lineages**, not just the lineage of the width currently being trained.

Use roughly 80/10/10 train/validation/test by independent related-game groups where feasible. The exact counts will depend on group sizes and inherited exposure. Relabeling a previously trained position does not make it fresh. Keep inherited validation separate from genuinely fresh validation; reserve fresh starts and subsequent games for final confirmation.

Hash both full game state and evaluator inputs. Identical feature inputs can have different history-dependent search values; do not silently average these as if they were ordinary repeated labels. If their targets depend on state the network cannot observe, quarantine them from scalar static-value supervision or retain them only for a clearly identified diagnostic. Search still handles terminal and repetition rules separately.

Target mining and calibration use training data. Validation gets a prespecified representative sample plus prespecified diagnostic strata. Test labels can be prepared by the frozen pipeline, but its predictions and errors stay sealed until the candidates are selected.

### Screen broadly and label approximately 40 roots per game

The initial allocation is **up to 32 representative roots plus up to 8 challenge roots per game**: at most 64,000 roots for 1,600 games, before overlap and exclusions. These are collection quotas, not training probabilities.

Representative roots should span game progress, remaining material/piece count, royal count, side to move, and ordinary strategic configurations. Use randomized stratified sampling with a recorded seed and a spacing preference, initially around eight plies. Short games or eventful regions can yield fewer samples. Count actual distinct groups and positions rather than filling quotas with near duplicates.

Challenge roots should mix strategic gaps, same-board teacher disagreement, budget instability, royal danger, promotions, surprising exchanges, confident outcome failures, and rare but relevant structures. Reserve at least one quarter of the challenge slots for random candidates within these structural strata, so the sampler is not entirely controlled by existing evaluators' errors. Cap deliberate range two-mover targeting at one quarter of the challenge allocation—roughly 5% of total training mass under the sampler below. Natural occurrence in ordinary positions is not capped.

Within a game, avoid taking eight nearly identical positions from one disaster. Usually select one or two roots per episode, with different episodes spread through the game. Earlier mate precursors are candidates for fresh search, not automatically mate-labeled examples. Do not extend a terminal label backward by a fixed 23-ply rule.

Compute cheap static predictions for both parents on the broad index if affordable. Run quiescence and deeper screening only on a bounded candidate reservoir; even quiescence can be expensive here. Saved scores and nearby-score gaps can propose that reservoir. Record what was available at selection time and the sampling probabilities where known.

### Use a frozen primary teacher and a genuinely different challenger

Screen 384v4.5, 512v3, and a credible handcrafted evaluator on training-only audit positions under the same corrected engine/search build. Choose a primary on evidence from corrected-rule play, label stability, and the analysis budget. Do not assume 512 is a stronger teacher because it is wider, or choose a teacher separately for every game according to its winner.

Use the other strong parent as a challenger. Include the handcrafted evaluator on a smaller diagnostic subset when it offers a materially different view. Its opinion gets no special authority, especially if it shares the same old material table. Three related NNUEs should not outvote one independent signal by sheer checkpoint count.

Freeze teacher content hashes, search configuration, material bindings, engine revision, and budgets before collecting the final training labels. Do not let live tournament leader changes silently change the dataset during the eight-model comparison.

### Spend extra search on a quality core

Use staged analysis, with this concrete initial workload:

- **Broad labels:** up to 64,000 roots with the primary teacher.
- **Paired audit:** 8,000 positions with both teachers at a larger matched budget: initially 6,000 broad roots and 2,000 generated children. Allocate half to a stratified random audit and half to high-interest positions within these sources. The random half measures how often the targeting rules miss errors.
- **Deep core:** 2,000 of the paired positions with both teachers at a still larger matched budget. Include random controls, unstable evaluations, narrow defenses, and shared confident failures, with a reserved child-position subset.
- **Alternative moves:** from 2,000 training/audit roots, generate up to three legal sibling positions, including a strong move, a credible alternative or defense, and a tempting move the parent favors. This adds at most 6,000 child positions, independently labeled.

Sample challenge positions for the deep core rather than taking only the largest disagreement. Some differences disappear immediately; some remain uncertain however long we search. Stop escalating an unresolved root at its budget cap and retain the uncertainty. Deeper is evidence, not proof.

Each alternative branch remains in its source group's split. It receives no inherited game result. Where a few forced tactical plies are used to reach a quieter position, preserve the exact continuation and perspective; label that resulting position itself. Do not attach the original root's value to all leaves or dump transposition-table values into the dataset as exact scores.

Reserve the 2,000 paired-audit child slots as two children from roughly 1,000 training roots. Reserve, initially, 400 deep-core slots for 200 such pairs; the remaining 1,600 deep slots cover broad roots. This funds the across-budget verification required by D inside the stated budget. All 6,000 children can contribute scalar labels, but only verified, stable pairs enter ranking supervision. If fewer pairs qualify, report the smaller pool rather than silently relaxing verification.

All four regimes receive the same new positions and labels. D alone adds pairwise supervision using the branch relationships. Thus D does not gain an exclusive better dataset.

### Illustrative compute budget

First run a roughly 1,000-root pilot, spanning phases and challenge types, to measure completed depths, nodes, label changes, and cost. The following budget is arithmetic for planning, not a performance measurement:

- 64,000 primary searches × 10 seconds: **177.8 CPU-hours**.
- 8,000 paired audits × 2 teachers × 30 seconds: **133.3 CPU-hours**.
- 2,000 deeper audits × 2 teachers × 120 seconds: **133.3 CPU-hours**.
- 6,000 child labels × 10 seconds: **16.7 CPU-hours**.
- Total additional search allocation: **461.1 CPU-hours**.

The later tiers are additional searches; this total does not assume checkpointable search or subtract the earlier work. At four one-core workers and 75% useful utilization, it is about **6.4 elapsed days**. It excludes training, feature export, startup overhead, pilot work, optional continuations, and game generation. Scaling the root counts down is safer than preserving the counts with labels that never complete useful search iterations.

The pilot must compare fresh search with the existing saved-search budgets. A nominal ten-second label is not automatically an upgrade over a saved evaluation. Retain or upgrade valid stronger evidence rather than blindly replacing it with a shorter search. For controlled teacher comparisons, use matched resource budgets and log completed nodes and depth; a fixed nominal depth is not equivalent work across this game's phases. A node-budget mode could improve reproducibility but is not currently exposed by `analyze_position`.

### Construct targets without erasing disagreement

For each teacher identity, use one eligible best completed observation at the intended budget tier. Do not count three depths from the same teacher as three independent votes. Store all depths to measure drift.

When compatible teachers agree reasonably, average their mapped probabilities with predeclared weights. Start with equal weights among distinct validated teacher families rather than per-game winner weights. A single good primary label remains usable. When teachers strongly disagree, obtain more evidence; if the dispute persists, record the range and give it less influence. The midpoint can be a soft training target, but must not be described as a confidently drawn position.

Use one shared, symmetric score-to-expected-result mapping for the first comparison, calibrated on training groups only. A scalar value represents expected score `P(win) + 0.5 P(draw)`, not a separate win/draw/loss distribution. Do not import chess's scale of approximately 400. Check calibration by teacher and phase; if teachers have materially different scales, fit regularized teacher-specific temperatures on training groups before averaging. With few independent games, avoid elaborate phase-dependent calibration curves.

The student's scale stays fixed across the four regimes. Record both original scores and mapped targets. Do not silently use a fitted probability temperature to change the engine's operational score scale, mate encoding, or pruning thresholds.

For mate claims, distinguish terminal rule outcomes, search claims, and independently verified forcing lines. Exclude terminal positions from ordinary NNUE supervision. A reproducible, nonterminal forcing result can use a bounded target such as 0.99/0.01; an unverified claim should be softened or withheld. Cap explicit mate-claim mass initially at 2% of all draws, within the challenge stratum. Never regress the integer mate score as ordinary material. Earlier positions keep their own finite or uncertain labels.

### A common training sampler

Use **70% representative, 20% challenge, and 10% alternative-branch draws**. The exact row counts do not determine these masses. Within each stratum, select related-game group, then game, then phase/episode, then position. Within a minibatch, prefer distinct groups when available. Preserve both colors rather than forcing an artificial 50/50 outcome distribution.

Within representative draws, allocate 80% to freshly labeled roots and up to 20% to eligible saved evaluations from the corrected-rule games. This caps the cheaper saved-label pool at **14% of total draws**. Saved-label eligibility requires a valid score, a completed search, compatible provenance, and no known contradictory stronger evidence. If it fails these checks, use the position only after reanalysis. Redistribute missing source/stratum mass explicitly and report the actual mixture.

Training weights should implement this hierarchy once. Do not both sample proportional to a priority and multiply the loss by that same priority unless the intended squared emphasis is explicit. Regimes A and B have no parent-error multiplier. C adds its separately defined confidence factor; D normalizes auxiliary pairs by root. None inherits the v5 error weighting automatically.

Track distinct groups, games, episodes, positions, and feature hashes drawn at each window; exposure counts; mass by source/phase; and duplicate/correlation diagnostics. An importance-weight effective sample size is useful, but does not turn correlated plies into independent games.

### Add future diversity without creating another collection experiment

Continue accumulating corrected-rule games from varied legal starts and a small mixture of credible evaluators. For a bounded portion of training-only starts, choose among competitive alternative moves and continue under ordinary search. Do not randomize every move or fabricate boards by deleting pieces. Prefixes before deliberate exploratory blunders should not inherit the subsequent outcome as if ordinary best play produced it.

For this first v6 comparison, freeze one collection snapshot. Additional games belong to a future refresh or a reserved confirmation pool. Short branches are sufficient for value labels; complete a bounded subset only when additional reliable outcomes justify the cost. Do not wait for thousands of new full games before trying v6.

## Preventing inherited range two-mover overvaluation

Correct rules remove the original move-generation problem, but they do not retrain a network or retune a material table. The inspected 384v4.5 and 512v3 descriptors have identical piece tables: HookMover 6,237, Capricorn 2,970, Tengu 2,494.8, Peacock 1,663.2, with Pawn 1. These are **baseline entries, not measured true values or the complete NNUE evaluation**; the residual may already offset some of them. [384 parent descriptor](../../models/nnue-v4.5/NNUE_W384_v4.5/model.json), [512 parent descriptor](../../models/nnue-v3/NNUE_W512_v3/model.json).

There are three distinct possible sources of persistence:

1. The fixed material baseline contributes an old prior.
2. The learned residual and related teachers may share an old prior.
3. Search also uses material values in ordering and tactical eligibility decisions, so correcting the final NNUE score cannot necessarily fix all search consequences.

The code confirms the residual-plus-material structure and separate search uses of material. [NNUE material implementation](../../src/nnue/mod.rs), [material definition](../../src/eval.rs), [search implementation](../../src/search.rs).

I would retain the fixed baseline in A as a control and remove it from the final evaluator in B–D. Search ordering and tactical eligibility keep a separate, identical material configuration across the four regimes. This isolates removing fixed material from the evaluation while acknowledging that it does not fix every material-dependent search choice. A blanket percentage cut to four pieces is not established by the rule change.

Before finalizing the migration, use the training audit to check:

- Residual error and outcome calibration versus each side's counts of affected pieces, conditional on phase, other material, royal safety, and mobility context.
- Natural exchange sequences and legal alternative moves involving those pieces, including cases where retaining one is actually correct.
- Confident errors when affected material favors the eventual loser, compared with similarly large errors involving other piece families.
- Saturation/clipping and whether the residual has enough representational range to compensate.
- Whether important defensive/capture moves are missed by material-dependent search eligibility, even after the static score is corrected.

Use group-held-out diagnostics and matched positions where possible. Count correlations alone do not identify a causal piece value: pieces survive, promote, and get traded non-randomly. Removing a piece from an arbitrary board is also not a valid ground-truth valuation experiment.

If these checks show a hard search bottleneck, make a **single common prerequisite search correction** before freezing the v6 comparison, then regenerate affected labels consistently. Do not quietly apply different search-material tables to different regimes. Evaluation representation differs explicitly between A and B–D; search configuration does not. Any later material/search retune needs its own playing validation.

Prefer preserving the existing ability channels to reuse the parent backbone. The repository deliberately preserves some historical feature encodings for checkpoint compatibility; an ability channel is not an oracle describing current legal reach. Audit missing identity/promotion information before removing material, as discussed below. A required feature addition must be versioned and shared by all four regimes, rather than hidden inside the pure-NNUE candidate. [Feature implementation](../../src/nnue/features.rs).

## Should fixed material be retired in favor of pure NNUE?

**Yes as a direction to test; not as an unvalidated deletion from the current models.** In this report, pure NNUE means that the network predicts the entire nonterminal static score. No fixed material score is added afterward. Material values can still exist separately for move ordering and other search heuristics, and game rules still determine terminal outcomes.

The current system is already an NNUE. An additive fixed term does not prevent it from valuing a piece lower: the residual can cancel that term. If the inputs contain all the relevant information and the model has sufficient capacity and output range, `M0+r` and a learned total score can express the same evaluations. The reasons to consider removal are optimization, inductive bias, score saturation, and the work spent repeatedly learning cancellation—not a claim that residual networks cannot learn material.

Our old, unusually large material priors make this a more relevant experiment than it would be for a well-calibrated baseline. On the other hand, 1,600 related games are not a huge independent training population. A material prior supplies useful structure, especially for rare pieces. Pure NNUE can still learn exactly the old overvaluation if every label contains it. Better target evidence remains necessary.

### Check that the inputs are sufficient

The existing features encode movement abilities, occupancy, and royal status, with no general piece-identity channel. Different piece/status combinations can map to identical features. For example, the material code distinguishes natural FreeKing from promoted FreeKing, while feature generation derives their channels from the current movement configuration. This establishes a potential mismatch with the old evaluator, not proof that the old valuation difference should be preserved.

Audit feature-equivalent pieces for differences in legal future behavior: promotion eligibility, promotion destinations, royal succession, and relevant provenance. If two positions are strategically equivalent, they need not retain different handcrafted values. If they have different legal futures but identical inputs, a direct total-score network cannot learn that distinction from those inputs alone. The fixed material path may previously have supplied a partial identity signal.

Prefer the smallest justified input extension, with old channels copied unchanged and new embedding rows initialized to zero. Give A and B–D the same extension so representation comparisons have the same information. Version the schema and verify warm-start parity before training. Do not reflexively add a full piece-square identity table for every exotic piece; that can greatly increase data and memory requirements. This audit is a design prerequisite, not evidence that a large feature redesign is required. [Feature construction](../../src/nnue/features.rs), [piece material and promotion exception](../../src/eval.rs).

### Migrate the parents rather than dropping their material term

Copy the parent's embedding and head into a temporary evaluator:

```
f_transition(x) = gθ(x) + alpha * M0(x)
initial gθ = parent's residual network
initial alpha = 1
final alpha = 0
```

Initialization preserves the parent's total evaluation, subject to verified import/export parity. Train against the fresh total-score/probability targets while lowering alpha in stages. A possible conversion pilot uses `1 → 0.75 → 0.5 → 0.25 → 0`, with bounded training at each stage. Advance only after training-only checks show that removing the next portion has not caused an uncontrolled score/calibration collapse. This is a proposed homotopy/transfer procedure, not a demonstrated capability of the current trainer.

Set a conversion ceiling, initially up to 262,144 draws per width, and report failures explicitly. Do not infer that pure NNUE is intrinsically weaker if the conversion never successfully reaches zero. Conversely, do not publish an alpha-positive checkpoint as pure NNUE. After reaching zero, finish ordinary training and select only fully migrated checkpoints for B–D.

For the first four-way comparison, create one converted starting checkpoint per width using the standard B objective; B/C/D all start from that same checkpoint. A receives an equal-draw prelude with alpha held at one and the same targets/optimizer settings. Reset optimizer state consistently at the subsequent regime-comparison boundary. This provides matched exposure while keeping the additional conversion work explicit. The shared prelude is preparation, not a fifth competing regime; it must not use held-out test data. The common prelude can still influence later results and belongs in the reported lineage.

Validate score range and clipping with the whole-score targets. The current data path constructs residual targets and clamps them; it cannot simply be reused with a renamed output. Persist target probabilities directly or construct total-score targets with the intended bounds, and make export/inference explicitly distinguish `residual` from `total`. Otherwise material may be subtracted during preparation or added twice in inference. [Current target construction and export](train.py), [quantized evaluator](../../src/nnue/mod.rs).

An alternative migration is a trainable linear material branch initialized from the old table, trained jointly with the nonlinear network. That makes all evaluation coefficients learned and preserves initialization exactly; it is still an additive branch rather than the strict single-output design requested here. It may be a useful fallback if direct conversion is unstable, but should not silently replace B mid-experiment. NNUE architectures can legitimately include learned linear components: Stockfish's architecture history records such a PSQT-like connection and its later removal, so “NNUE” does not itself prescribe one output decomposition. [Stockfish architecture history](https://github.com/official-stockfish/nnue-pytorch/blob/c625702757b58999611b006b5a03e3e1c5e142ea/docs/nnue_architecture_history.md).

### Deprecation should follow the result

If B beats A reproducibly under matched search, make total-score evaluation the default for newly trained networks. Preserve explicit residual-mode loading for v3/v4.5 parents and historical reproducibility. Keep terminal logic and search-material heuristics separate. If B ties A, prefer the choice with demonstrated calibration, reliability, and acceptable inference cost; architectural cleanliness alone is not a strength result.

## Four training regimes in detail

### Common notation and controls

Let `fθ(x)` be the total quantization-aware score: `M0(x) + rθ(x)` in A and `gθ(x)` in B–D after migration. Let `S` be the frozen student scale, `u = fθ(x)/S`, and `p = sigmoid(u)`. Let `q` be the frozen teacher expected-score target and `z` a reliable side-to-move game result in `{0, 0.5, 1}`. The formulas below describe mean losses under the hierarchical sampler.

Train all four regimes at both 384 and 512 widths, starting respectively from 384v4.5 and 512v3. Keep the backbone width/head layout, shared input information, label catalogue, splits, quantization policy, and search configuration identical. Evaluation-mode metadata must explicitly distinguish residual and total-score models. Compare regimes **within width** first: a 384-versus-512 result also includes a different parent and inference cost.

Use fresh optimizer state for each warm start, with identical initialization within a width. Loading exported parent weights is not an exact optimizer continuation. Keep sparse SGD for the embedding and Adam for the small dense head initially; dense adaptive state for the full feature table is a separate memory/performance decision. Do not migrate trainers or optimizers solely because a chess recipe uses a different one.

### A and B — The two standard regimes

Both use the same conventional outcome-mixed cross-entropy objective:

```
y = (1 - beta_i) * q + beta_i * z
LA = LB = BCEWithLogits(u, y)

A: fθ(x) = fixed_material(x) + learned_residual(x)
B: fθ(x) = learned_total_score(x)
```

Start with `beta_i = 0.15` for eligible game outcomes and zero otherwise. This is an outcome share, not Stockfish's teacher-lambda convention. Keep beta fixed during the first comparison so target mixing does not become another annealing variable. A and B have no difficulty multiplier.

Eligible results include correctly recorded terminal wins/losses and classified rule draws under the current rules. A move-limit stop, aborted game, unknown draw reason, timeout, or unverified evaluation-based adjudication should not automatically become a reliable position-value target. Keep its teacher target while setting beta to zero. Branches without independent completed outcomes also use beta zero. If an outcome remains usable but noisy because of policy strength, the modest mixture is the protection; do not retroactively choose only trajectories where the winner looked right.

Cross-entropy has normalized-score gradient `p-y`; squared probability error has an additional sigmoid derivative. Thus both standard regimes can more readily correct a confidently wrong student. The outcome term supplies information not obtained merely by asking related teachers to agree. Keep A as a credible modern control rather than disadvantaging it with a weaker objective or older labels.

**A's question:** how far can clean labels and a standard objective improve the existing representation? It preserves its additive material prior and convenient initialization.

**B's question:** does letting the network learn the whole score improve on A? It avoids a mandatory old material contribution, but needs a successful warm-start migration. Compare A/B at the same post-migration training exposure as well as total compute, and document any extra migration cost. Migration is part of the practical change; A/B is not a theorem about the two function classes.

For both, noisy labels can move the network too strongly, and outcomes from long games provide weak credit assignment. Use stable logits, conservative learning rates, game-level weighting, and calibration diagnostics. The 15% result share is a proposed starting value, not an established optimum.

### C — Pure NNUE with robust score correction, experimental

For finite labels, define `t = logit(clip(q, 0.01, 0.99))`. Let `H1` be Huber loss with transition at absolute normalized-score error 1: quadratic `0.5 e²` inside that interval and linear `|e|-0.5` outside.

```
LC = c_i * [0.8 * 0.25 * H1(u - t) + 0.2 * 2 * (p - q)^2]
```

The small probability term preserves alignment with the value target; the robust score term gives a bounded but non-vanishing gradient for severe confident errors. Unlike probability MSE, it can push directly against a saturated prediction. Unlike raw-score MSE, it does not make arbitrarily large teacher numbers dominate. The 0.25 factor matches local curvature around balanced predictions (`p ≈ q ≈ 0.5`) to the other losses before confidence weighting; their curvature differs in the tails.

For paired-audit positions, start with a fixed confidence heuristic:

```
c_i = max(0.25, 1 / (1 + (teacher_gap/0.15)^2 + (budget_drift/0.15)^2))
```

Use `c_i=0.75` for eligible single fresh labels and `0.5` for eligible saved labels when these measurements are missing. Missing uncertainty is not zero uncertainty. Normalize the mean confidence within each stratum for overall loss scale, and report the resulting changes in game/group mass. The floor prevents discarding every difficult position. This heuristic is not a calibrated probability that a label is correct; agreement among biased teachers can still be wrong.

Use `2*(p-q)^2` with the accepted bounded target for mate claims, outside the Huber term. Do not transform an integer mate distance into `t`. C uses no outcome mixture, making its main question whether saturation and unreliable score precision were obstructing useful correction.

Risks: clipping the target logit sacrifices distinctions among strongly favored positions, and score-space correction can change search behavior even with a good probability fit. Confidence weighting may also prefer easy consensus. Check tail-score behavior and tactical outcomes explicitly; do not add strong imitation of the original parent, which would anchor precisely the bias we want to correct.

### D — Pure NNUE with sibling ranking, experimental

Use B's pure total-score network and value loss on the common corpus, plus a small auxiliary loss on freshly analyzed alternatives from the same root. Let `va` and `vb` be student scores at the sibling positions, converted to the **same root player's perspective**, and `sa`, `sb` the corresponding teacher scores in that perspective.

```
q_pair = sigmoid((sa - sb) / S)
Lpair  = BCEWithLogits((va - vb) / S, q_pair)
LD     = LB + 0.10 * mean_per_root(Lpair)
```

For immediate children, root-perspective value is the negative of the child's side-to-move value. For longer continuations, use actual side-to-move identity, not an assumed parity. Reject pairs with unknown orientation, unresolved search bounds, or unstable ordering across the verification budgets. Prefer relatively quiet sibling positions for the ranking term; tactical children can still receive their own scalar labels.

Start by retaining pairs with a meaningful expected-score gap, for example 0.10, and consistent ordering in the paired/deep audit. Do not turn tiny numerical differences into confident hard labels. Average pairs within a root so three or ten enumerated alternatives do not multiply its influence. Freeze teacher targets; the current student should not continually redefine which move wins during the comparison.

Value anchors are essential. Ranking alone leaves the scale and absolute value underdetermined and cannot train a useful search evaluator reliably. Also, the ranking auxiliary does not require changing inference: it updates the same scalar network. Examples should include choices such as trading an overvalued piece for a safer royal position, but also ordinary development, attacks, promotions, and defenses.

Risks: teachers can agree on a wrong ordering; static child values can be misleading in unstable tactical positions; and auxiliary gradients can overwhelm value fitting. The initial 0.10 coefficient and strict pair eligibility keep this a bounded addition. Measure pairwise accuracy and deep-search regret, but require actual playing improvement.

## Optimization, annealing, and checkpoint selection

For the first comparison, keep optimization mostly common so the four hypotheses remain understandable. A useful initial recipe is:

- Effective batch **8**, with microbatch accumulation as needed and distinct related-game groups when possible. Larger batches can be a later efficiency choice; do not borrow chess's huge batches for our thousands of active features per position.
- Sparse embedding SGD and dense-head Adam, starting near the established conservative rates `1e-4` and `1.5e-4` after a gradient-scale pilot.
- A shared ceiling of **16 × 65,536 = 1,048,576 position draws**, with validation every 65,536 and an early check at 32,768. This is a comparison budget, not sixteen full dataset passes.
- Warm the learning rate from 10% to its peak over the first 32,768 draws, then cosine-decay to 10% of peak at the ceiling. Keep the untrained parent as a deployment fallback. Only fully migrated, zero-baseline checkpoints are eligible to represent B–D.
- Save complete optimizer, sampler, schedule, and best-model state at update boundaries. Resume exactly; changing the recipe creates a new run.

Loss normalization matters. The current implementation multiplies probability MSE by `8*(S/1000)^2`. Multiplying each normalized loss above by the common factor `4*(S/1000)^2` matches the old objective's local curvature around balanced predictions (`p ≈ q ≈ 0.5`) and gives A/B/C comparable curvature there before C's confidence factor. This does not match their gradients across the entire score range. D's auxiliary coefficient still needs its gradient contribution checked. Do not reuse learning rates across differently scaled losses without this check. [Current probability loss and optimizer setup](pilot_fit.py).

Use a short training-only pilot to check finite gradients, clipping, gradient magnitudes, and update sizes. Permit a bounded learning-rate adjustment to make objectives numerically trainable, record it, and freeze it before the full comparison. No need for a broad optimizer sweep. The proposal does not assume CUDA support; the [GPU plan](GPU_PLAN.md) records the separate implementation work required for it.

A fixed ceiling avoids allowing one regime unlimited additional optimization while another stops early. It also does not require deploying the final weights. Select the best exported quantized checkpoint on a common validation score, initially:

```
J = 0.85 * game_balanced_Brier_to_fresh_teacher
  + 0.15 * game_balanced_Brier_to_eligible_outcome
```

Each term is normalized on its own eligible representative validation groups. Freeze the outcome term's inclusion and weight before training; if the outcome audit finds too little reliable coverage, use teacher-only selection consistently and document that limitation. Do not compare raw numerical A/B/C/D training losses to choose a regime. Keep targeted tactical and alternative-move metrics as separate diagnostics rather than allowing a handful of handpicked positions to dominate J.

The two terms measure different things: teacher fit and policy-outcome prediction. Neither is perfect-play ground truth. Record both separately, plus calibration, confident-wrong rate, probability MAE, raw-score tail behavior, mate false positives, and affected-piece/control-piece slices. Validate the quantized export throughout, not only the floating model.

Use one common seed for the initial eight runs. If resources permit, repeat the leading regime and A at each relevant width with two additional seeds. Otherwise describe the result as an eight-run screen with one seed per cell. Different widths have different parents, so their difference is not a pure width experiment. D also consumes pair evaluations; record total forward/backward evaluations and CPU/GPU time in addition to position draws, and report an equal-compute checkpoint if its extra work is material.

## How to decide whether v6 is better

### Separate development from confirmation

Use validation and a development match pool to select candidates. Reserve an untouched labeled test set and a fresh match-start pool for final confirmation. Once a test result informs another change, that test is development data and needs a replacement for a fresh claim.

In matches, hold the engine/search build, time control, threads, hash policy, and hardware constant. Play paired colors from the same varied starts. Compare each candidate with its own parent, a common strong reference, and a small diverse opponent panel. A single favorable matchup against the teacher is insufficient.

Fixed-time strength is primary because width changes inference speed and therefore search work. Fixed-node tests are a useful secondary view of evaluation quality. Include a longer-search confirmation on finalists so a gain at one shallow budget does not conceal a serious tactical weakness. Keep operational score calibration fixed unless recalibration is explicitly part of a separately recorded change.

An initial screening allocation of roughly 100–200 opening pairs per promising comparison may catch large regressions; it is not enough to promise detection of a small improvement. Continue finalists with a prespecified statistical stopping rule or a larger fixed sample. Estimate uncertainty at the opening/paired-game level, not the sampled-position level. With repeated looks, do not treat ordinary fixed-sample confidence intervals as a valid sequential acceptance rule.

Record crashes, illegal moves, time losses, unknown terminations, draw types, and opening-family sensitivity alongside match score. Randomized pairing and fixed opponent weights help prevent tournament scheduling changes from masquerading as network gains.

### Read the pattern of results

- If A improves clearly, the common data refresh and conventional outcome-mixed objective may already be valuable.
- If B improves over A, learning the total score is promising. Check migration and input parity before attributing the difference solely to representational bias.
- If C improves, score saturation/label reliability deserves more attention. Check that it did not merely rescale evaluations favorably for one search budget.
- If D improves with similar value error, decision-relative information may be more useful than another increment in absolute teacher fit.
- If all regress, first inspect shared labels, calibration, search interaction, and parent compatibility before enlarging the networks or collecting more of the same positions.

These are interpretations of joint interventions, not one-factor causal proofs. Each regime is a practical package. Further ablations belong after identifying a promising package, not inside an uncontrolled expansion of the initial four-way comparison.

## Implementation work and order of execution

The current repository already has immutable snapshots, sharded feature storage, grouped splits, content-addressed models, cached teacher labels, bounded weighting, quantization-aware training, and resumable optimization. Reuse these mechanisms.

The main missing work is more specific:

1. **Freeze the common experiment:** current-rule game snapshot, union-of-parent exposure index, group splits, teacher hashes, source quotas, and validation protocol.
2. **Extend analysis evidence:** static and quiescence scores, explicit search configuration, termination/bound status, and independently verified alternatives. The current CLI emits completed depth, score, best move, nodes, and elapsed time; it does not export the proposed complete evidence or a node-limit interface. Its callback receives root lines but does not emit them, and root-line scores must not be assumed exact MultiPV values. [Analyzer](../../src/bin/analyze_position.rs).
3. **Run the small collection pilot:** establish useful budgets and check whether saved or fresh evaluations are stronger evidence. Resolve any material/search prerequisite before freezing labels.
4. **Collect the shared core:** broad roots, paired audits, deep verification, and legal alternatives, with incremental checkpoints and a resumable queue. Keep unresolved disagreements explicit.
5. **Build the common corpus:** robust target provenance, outcome reliability, branch links, source caps, and sampling audit. Do not change the live tournament or automatically admit models as a side effect of preparation.
6. **Implement the output modes and four regimes in the shared trainer:** A/B share stable BCE and per-row beta; B–D need the tested migration and explicit total-score export mode; C needs normalized Huber/confidence; D needs paired sampling and perspective checks. Keep one parameterized trainer rather than four copied scripts.
7. **Run the eight-model screen, then confirm finalists:** use the same corrected search and compare exported models against their parents. Publish the final recipe, actual compute, source/split counts, and measured results.

Regression coverage should target score orientation, sibling parity, outcome eligibility, mate-versus-finite labels, bound handling, actual source masses, union-of-parent leakage, quantized arithmetic, and checkpoint recovery. After the winning recipe is decided, remove losing experimental paths according to repository policy. A later major eval/search merge should follow the existing history-freeze policy; this research report itself changes no engine behavior.

## Decisions I would make now

Use the approximately 1,600 corrected-rule games, retain their dense archive, and start with roughly 64,000 deliberately selected roots plus up to 6,000 alternative positions. Put the expensive analysis into a representative-and-targeted quality core. Preserve a small capped saved-label pool for breadth.

Start from 384v4.5 and 512v3 and compare A/B/C/D at both widths, with identical data and search configuration. A retains fixed material; B–D learn the total evaluation. Prioritize the A-versus-B representation comparison and D as the more ambitious experiment. Do not promote a teacher because it won the particular game, hardcode lower range two-mover values, or optimize primarily on mate-rich endings.

The aim is to teach corrected-rule consequences that the parents and short search systematically miss, while retaining ordinary positional judgment. More rows help only when they add coverage, independent evidence, or better targets.
