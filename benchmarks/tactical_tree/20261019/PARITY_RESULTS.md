# NNUE search-depth parity survey — completed 2026-10-18 Pacific

All 120 position/model jobs completed, with no failures. The supervisor released both CPU leases. Verified every requested depth, common search configuration, per-position replay hash/history across models, and 47 source/binary/checkpoint/game hashes. There were 528 independent fixed-depth requests, containing 1,440 completed iterative-deepening iterations. No root terminal shortcuts or mate-range scores occurred. Summed request elapsed time was 6,844 seconds (about 1.90 CPU-hours of single-threaded requests; not wall duration or measured CPU accounting).

## What the evidence supports

Large odd/even differences are common in this small recent-game sample and larger in the v4.5/v5 models than in 512v4. They are not confined to the originally selected tactical example. Same-parity changes are usually much smaller. This establishes an evaluation/search-consistency issue worth investigating; it does not establish which generation plays better or which score is correct.

The strongest diagnostic is that a large part of the difference already exists in freshly computed static evaluations on the same board with opposite turns, and persists after legal quiet moves. This cannot be explained solely by selecting the best of many noisy leaf moves, nor solely by a stale incremental accumulator. It does not establish that the whole turn gap is wrong: tempo, threats and forced replies can have real value. Search and quiescence sometimes suppress an enormous static turn gap, so turn dependence is not sufficient to predict oscillation.

## Sampling and metric

Twelve distinct completed recent games involving v4/v5 were hash-selected before scores were examined, with one randomly located position per game: four early, four middle, four late by relative game progress. Three known diagnostic positions are kept separate. All eight available v4–v5 checkpoints used the same replayed positions, full inherited histories, engine revision and saved search defaults (q depth 2, PathAware, sibling mode 0). Every job independently searched depths 1–4; three representative and all three diagnostic positions also searched depth 5. No elapsed deadline was imposed.

The main offset is `abs(S3 - (S2+S4)/2)`, in native evaluation units, with all scores in Black's perspective. This removes a linear depth trend but does not distinguish clean alternating oscillation from every possible tactical discontinuity. Adjacent-change medians pool |S2-S1|, |S3-S2|, |S4-S3|; same-parity medians pool |S3-S1| and |S4-S2|. Each model has the same number of positions. Counts are sample counts, not precise population prevalence estimates. Twelve games are the independent sample units, not 96 independent model/position observations.

## Representative sample

| Model | Median offset | Range | >=1,000 | >=3,000 | >=5,000 | Median adjacent change | Median same-parity change | d3/d4 move changed |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 512_v4 | 451.8 | 61.5–1,145.0 | 2/12 | 0/12 | 0/12 | 403.0 | 149.0 | 6/12 |
| 384_v4.5 | 1,086.8 | 19.5–5,281.0 | 7/12 | 2/12 | 2/12 | 856.5 | 121.0 | 9/12 |
| 512_v4.5 | 1,477.0 | 2.0–6,792.0 | 8/12 | 2/12 | 2/12 | 1,203.0 | 130.5 | 9/12 |
| 1024_v4.5 | 1,075.2 | 6.0–5,949.0 | 7/12 | 2/12 | 2/12 | 818.0 | 99.5 | 10/12 |
| 1536_v4.5 | 1,378.8 | 12.0–5,585.0 | 7/12 | 2/12 | 1/12 | 1,054.5 | 61.5 | 7/12 |
| 512_v5a | 1,703.8 | 0.5–8,160.5 | 8/12 | 4/12 | 2/12 | 1,046.0 | 195.5 | 8/12 |
| 512_v5b | 2,105.0 | 0.0–7,268.5 | 9/12 | 5/12 | 2/12 | 1,378.5 | 139.5 | 7/12 |
| 512_v5c | 1,318.5 | 0.0–7,351.5 | 6/12 | 3/12 | 1/12 | 725.5 | 157.0 | 9/12 |

The odd-depth estimate was worse for the root side than the mean of neighboring even depths in 85/96 model/position comparisons (two ties and nine opposite signs). Thus this is usually the opposite of a simple “last mover looks optimistic for the root” explanation. Static turn dependence favored the side to move in 117/120 total probes. For quiet leaves, changing which side is to move changes which side receives that learned advantage.

## Deliberately selected diagnostics

| Model | Median offset | Range | >=1,000 | >=3,000 | >=5,000 | Median adjacent change | Median same-parity change | d3/d4 move changed |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 512_v4 | 1,349.5 | 1,094.5–1,644.5 | 3/3 | 0/3 | 0/3 | 1,342.0 | 80.0 | 2/3 |
| 384_v4.5 | 4,397.5 | 3,631.5–7,481.5 | 3/3 | 3/3 | 1/3 | 4,395.0 | 20.0 | 3/3 |
| 512_v4.5 | 3,980.5 | 3,543.5–5,601.5 | 3/3 | 3/3 | 1/3 | 3,975.0 | 78.5 | 3/3 |
| 1024_v4.5 | 3,543.5 | 3,149.5–6,258.0 | 3/3 | 3/3 | 1/3 | 3,535.0 | 67.5 | 1/3 |
| 1536_v4.5 | 2,980.0 | 2,333.0–5,828.0 | 3/3 | 1/3 | 1/3 | 2,995.0 | 190.0 | 3/3 |
| 512_v5a | 4,022.0 | 3,428.0–7,337.0 | 3/3 | 3/3 | 1/3 | 4,032.0 | 86.5 | 3/3 |
| 512_v5b | 3,751.5 | 3,290.0–7,560.5 | 3/3 | 3/3 | 1/3 | 3,752.0 | 15.5 | 3/3 |
| 512_v5c | 3,878.5 | 3,449.0–7,390.5 | 3/3 | 3/3 | 1/3 | 3,835.0 | 68.0 | 3/3 |

## Concrete examples

All scores below are from Black's perspective; ply is the position before that numbered recorded move. Coordinates in derived result JSON are zero-based engine file/rank coordinates and include full routes, promotion and intermediate squares.

**Slot 1337, ply 1024 (known diagnostic; White to move).** 384v4.5 reproduces the earlier sequence exactly: +712, -6,776, +695, -6,797, +647. 512v5b gives +380, -7,297, +264, -7,296, +92. Its same-parity changes are -116, +1, -172, dramatically smaller than the ~7,500 alternating difference. 512v4 gives +1,551, +238, +1,276, +125, +983: still oscillating, much less.

For 512v5b the root static score is -7,319; changing only the turn gives +642. After its legal depth-3 chosen move (28,10)->(7,10), the static score becomes +408; flipping the turn back on that child gives -7,392. Material remains -2,255.92 throughout. Its depth-4 choice (28,10)->(20,10) gives +482 versus -7,402 on the flipped child. Standalone q2 returns the unchanged child static values. A quiet move causes a large alternating evaluation jump even without a material change; the same-board turn dependence closely tracks it. This is evidence for a learned turn/continuation inconsistency, not proof of the position's true value.

**Slot 1399, ply 450 (independently selected middle-game case; White to move).** 512v5b gives +2,760, -4,797, +2,632, -4,476, +2,471. Static root -4,722 versus +2,873 with turn flipped. After the legal depth-3 selected move (21,34)->(20,33), material is unchanged and static becomes +2,760 (flipped: -4,797). Standalone q2 returns +2,760 with one node. This confirms the severe behavior outside the handpicked set. 512v4 instead gives -6,425, -4,981, -5,942, -5,299, -5,614. Its smaller oscillation does not prove its consistently negative assessment is correct.

**Slot 1373, ply 1180 (mild search parity despite a huge static gap).** 512v5b gives +8,247 at every depth 1–4, selecting the same full move route each time. Root static is +1,696 versus +8,213 with turn flipped; standalone q2 already gives +8,247. After the selected legal move (17,35)->(18,35), static and q2 are +8,247. This shows that the treatment of forced/tactical continuations can dominate the static turn gap. It does not prove qsearch fixes calibration or that +8,247 is correct.

**Slot 1403, ply 541 (another mild case).** 512v5b search scores are -5,633, -5,739, -5,693, -5,651, despite root static -1,942 versus flipped -5,787. Root q2 is -5,633. There is no large residual alternating search offset here.

**Slot 1459, ply 697 (horizon interaction, not clean alternation).** 512v5b gives -6,704, -6,541, -6,541, -23, -6,623. Depth 4 looks dramatically better but depth 5 reverses that result. This is why the offset statistic must be read with individual trajectories; a single extra depth can be misleading, but simply averaging depths would also be unjustified.

## What remains unproven

- Same-board turn flips are diagnostic counterfactuals, not legal consecutive game positions. Threats and zugzwang can justify large differences.
- The static probes evaluate fresh NNUE accumulators: the worker replays the board before loading the model and does not attach an incremental NNUE to its base state. This rules out stale incremental state as the sole explanation for those static gaps. It is not an exhaustive audit of inference, labels, pruning or search.
- `src/nnue/mod.rs` orders the two accumulators by side to move. `src/eval.rs` adds the signed fixed material value to that residual. `training/nnue/pilot_fit.py` fits individual teacher/outcome labels, without a parent/best-continuation consistency term. None of these facts alone is a bug.
- The standalone q2 probe lacks the actual AB leaf's previous-move context and uses its own capture defaults. Its output is not an exact production-leaf ablation. We inspected legal first continuations, not complete principal-variation leaf traces.
- Different widths, training data/objectives and numerical calibration confound cross-generation comparisons. Same-width 512 comparisons still show the increase, so width alone does not explain it. Smoother scores are not stronger play.
- No mate-range transitions occurred; this survey says nothing quantitative about mate-label handling. Root candidate scores are heuristic ordering values, not certified multi-PV scores.

## Recommended next experiments (not deployed)

1. **Audit label and inference consistency before changing training.** On held-out games, compare trainer versus exported-engine evaluation, Black/STM sign conversion, fixed-material subtraction, and teacher-label residuals by teacher depth parity and turn. The current tests did not find an implementation failure, but this audit is a cheap way to distinguish a data/sign problem from learned calibration. Test on games outside this survey to avoid tuning to these examples.
2. **Relabel a bounded sample of actual search-selected leaves and nearby legal continuations.** Record the production qsearch endpoint/context, then get an independent teacher search from those boards, retaining both adjacent completed depths and disagreement. Search-selected leaves may differ from ordinary played positions. Do not simply reuse the suspect model's score as the new truth, and do not discard disagreement by averaging it away.
3. **Train a small continuation-consistency arm alongside the ordinary supervised objective.** For teacher-supported best moves, encourage parent and best-child values to agree in a common perspective (or negate when using STM scores). Keep independent teacher/outcome and terminal anchors, modest weighting, and exclude unsupported equality constraints on arbitrary played moves. This is a proposed adaptation, not a claim that Stockfish uses this exact loss. TDLeaf provides precedent for coupling learning to minimax search and leaf trajectories: https://arxiv.org/abs/cs/9901001 .
4. **A separate quiescence-alignment ablation.** Trace the actual AB leaf gate and q endpoints on severe and mild cases; compare a small controlled change only if it explains the difference. The mild examples justify checking alignment, not indiscriminately enlarging qsearch. Measure nodes/time, tactical outcomes and held-out play; do not accept a slower smoother curve by itself.

Use independent whole-game splits, ordinary and tactical held-out positions, calibration error, move stability and playing strength at both 1+1 and the intended 15+5. Preserve legitimate tempo differences. A constant turn subtraction, forced score symmetry or odd/even averaging is not supported by this evidence.

## Reproduction and provenance

- Research source: `10722c27828d89b668db9b8a8544445a7cc930b0`, parent `729314c703d0294eecf68756e5bd5f1ccb41d4c0`.
- Binary SHA-256: `25728f3cdff1d3b05ea3a0e83ddc9d68cabcb2ef06b2b627d2d330e5e4c24f7b`.
- Compiler: rustc 1.92.0, x86_64-unknown-linux-gnu; release build, `CARGO_BUILD_JOBS=2 cargo build --release --bin tactical_tree_worker`.
- VPS frozen config, model hashes, original game hashes, raw fsynced iteration events and summaries: `/opt/nnue-parity-20261019/`.
- Local private summary archive and analysis: `data/analysis/nnue-parity-20261019/`.
- Derived per-position rows, full selected routes, depth scores, nodes/qnodes/timing and aggregate metrics: `parity-derived.json` beside this report. Source games and model blobs remain private.
- `training/tactical_tree/PARITY.md` documents execution. `summarize.py` in the local private analysis directory reconstructs aggregates from downloaded summaries; it performs no search.

No production evaluation, checkpoint or training policy changed. The completed service released CPU leases and the follow-up monitor is paused after delivering this report.
