# NNUE training strategies

## Persistent strategic overvaluation

A network can recognize a material or positional advantage and still badly overestimate the side’s ability to convert it. Its search may endorse that judgment for hundreds of plies because the same evaluator scores the leaves. The training question is whether competent play can turn the advantage into a win, and which board features distinguish it from a genuinely winning position.

### Where to look

In a November 2026 screen of 3,764 corrected-rules games involving v7 or v7-search, we selected turns where the lower of the agent’s two consecutive search scores was between +10,000 and +11,999. We compared the opponent’s two preceding scores, all expressed from the focal agent’s perspective. When both opponent scores were at least +8,000, the focal side lost 0.6% of qualifying turns’ games (267 losses in 41,840 turn observations). When both were within ±2,000, the loss rate was 14.1% (11,764 in 83,465). Those groups covered 748 and 1,074 distinct games respectively. Adjacent turns share outcomes and evaluate different boards. Disagreement helps select positions for review. It does not supply their labels.

Slot **10560, ply 554**, is a starting point. The eventual loser’s raw evaluation was +10,199, its search score was +10,104, and the opponent’s preceding searches gave that same side +138 and +137. The game ended 57 plies later. Inspect the preceding stretch to establish how long the structure and disagreement persisted. Slots **10504 and 10678** illustrate a sampling trap. Both crossed the threshold at ply 322 from identical starts and move histories, and first diverged at ply 349.

### Build a pilot dataset

1. **Find episodes.** Use two consecutive +10,000 search scores as an inexpensive entry point. Trace backward and forward to measure how long the raw evaluation, search evaluation and opposing assessment diverge. Record the duration and the moves that change the strategic structure. Give priority to persistent disagreements in positions where the attacking side still needs to make progress. Include draws and wins as well as losses.
2. **Match successful conversions.** Start with roughly 50 suspect episodes and 50 episodes where a similarly valued advantage was converted. Match as closely as the corpus allows on checkpoint, material balance, surviving royals, phase and opponent strength. Compare how the attacker gains entry, whether the defender can repeat its formation, and what material it costs to open a route. Sample a few distinct boards from each episode and distribute one episode’s weight across them. Keep shared boards, move prefixes and opening families together when splitting training and evaluation.
3. **Play out a subset from the middle.** Restore the original position, side to move, repetition history and no-progress counter. Use a documented clock budget and several strong agent pairings, rotating who controls each side while keeping board colors fixed. Record wins, draws, losses and the plans that produced progress. Repeatedly replaying the same deterministic continuation supplies little additional evidence. Start here before spending the budget on deeper labels from the original evaluator.
4. **Assign the correction where it belongs.** If several strong agents struggle to convert the same structure, use their results as a soft training target, with less weight when there are few independent continuations. Apply it to representative earlier boards where the structure persists. If other agents convert reliably, inspect their plans and v7’s first missed opportunity. If one later mistake destroys a genuine advantage, concentrate the correction around that decision. Leave ambiguous examples available for further review.

### Decide whether the strategy helped

Warm-start a control and a candidate from the same checkpoint with the same training budget. Replace a small share of ordinary samples in the candidate with episode-weighted examples. The deliberately matched pilot is useful for comparing boards. Estimate absolute win probabilities on a separate sample with the tournament’s natural outcome frequencies.

On held-out opening families, check whether the candidate lowers its confidence in failed conversions **while retaining confidence in matched wins**. Also test whether it prefers continuations that made progress in the play-outs. A uniform reduction in score magnitude could improve calibration without improving a single move. Use tournament games to judge playing strength after these checks.
