# Research and previous-results appendix



## What was left in earlier work

The baseline already contains the high-value accepted work in `benchmarks/nnue_followup`: native SIMD dense inference, reusable accumulator/undo storage, exact i16 fused-row caching with i32 overflow fallback, bounded whole-move deltas, canonical feature rows, parked caches, capture/gate/attack memos and root accumulator reuse. `benchmarks/nnue_agent_speed_20261025` also established royal indexing. This sweep does not count any of those historical savings again.

There was still some juice in the older allocation and repetition proposals. The new iterator experiment removes the per-move NNUE subtraction Vec; direct movement output eliminates per-capability intermediate Vecs for common moves; and root-indexed repetition avoids walking the entire recorded game at every visited node. These are distinct from replacing the already-optimized dense first layer.

Previously neutral ideas remain conditional. Deferred accumulator updates need enough evaluations actually avoided; the earlier census found most evaluated frames never made a child, reducing the opportunity for residual reuse. Sparse absolute-input kernels lacked measured activation sparsity (the older census found only 0.25–0.55% zero inputs, with no zero four-input blocks). Stockfish's sparse-feature premise concerns changed inputs and its trained activation distribution, so sparse input does not automatically imply sparse hidden activations in our networks. Huge pages, transposed heads, fat LTO and incremental line occupancy had mixed or neutral integrated results. No historical multiplier is a new forecast for this baseline.

The new byte-dot second layer, fixed-arity cached-row updates, fused narrowing/fit check, quantization shift, output lookup and batched first-layer reduction were concrete attempts to revisit NNUE arithmetic or memory traversal rather than repeat old recommendations. Their individual measured results are in the screen table. Native compiler PGO was re-run because it previously helped the actual full engine; that is compiler training on game searches, not network training.

Historical source-derived estimates in `ENGINE_SPEED_ANALYSIS.md`—including large attack-map and move-generation projections—remain estimates. A new movement-class attack index is a plausible structural next step, but it must pay for mutation, clone and sweep/undo maintenance and beat existing warm memos. The present combined target-filter rewrite did not establish that gain. An early-exit royal-flight scan was included in the material-only package, but all seven benchmark agents have `lr_flight_k == 0`, so this sweep measures no benefit from that particular change.


## NNUE research and rejected transplants

Nasu's original 2018 NNUE description explicitly uses byte activations and byte weights after the transformer, including the small dense layer. That motivated the exact 32×32 byte-dot candidate. Its applicability is architectural; the original paper supplies no Taikyoku timing or strength evidence. Disassembly of our baseline already contains vectorized i32 dot products (`vpmovsxbd`, `vpmulld`, horizontal reductions), so describing the baseline as a scalar second layer would be incorrect. [Original NNUE paper, §2.8](https://www.apply.computer-shogi.org/wcsc28/appeal/the_end_of_genesis_T.N.K.evolution_turbo_type_D/nnue.pdf)

The Stockfish NNUE documentation and source support incremental transformer updates, quantized dense layers, SIMD layouts and sparse-feature techniques. Our baseline already implements several of those mechanisms. Stockfish's current accumulator work includes same-sign int8 feature pairs widened to i16 and load-time permutation; its own PR author reported a platform-specific gain. Our bounded cache stores fused i16 rows, accumulates i32 sums and often hits a whole-move delta cache, so that result cannot be imported as a gain estimate or a safe drop-in representation change. The dense kernels tested here are independent implementations; no external source was copied. [NNUE documentation](https://github.com/official-stockfish/nnue-pytorch/blob/568c7fbdff25412423947041ab3fa3f2037b0bac/docs/nnue.md), [Stockfish affine layer](https://github.com/official-stockfish/Stockfish/blob/ff7cee4244ec96ea488c402669e3674c0574dce6/src/nnue/layers/affine_transform.h), [accumulator source](https://github.com/official-stockfish/Stockfish/blob/ff7cee4244ec96ea488c402669e3674c0574dce6/src/nnue/nnue_accumulator.cpp), [int8-pair experiment](https://github.com/official-stockfish/Stockfish/pull/7225)

A recent NNUE dataset study suggests filtering unstable/tactical positions when preparing training examples. That is a data-quality experiment requiring retraining and fixed-time tactical/playing comparisons. It was read for context but no model training or dataset change was attempted in this speed sweep. [A Study of Proper NNUE Dataset](https://arxiv.org/html/2412.17948v1)


# Engine and paper research: classification recommendations

The strongest next experiments remove work while preserving our existing move routes, traversal order and search decisions. Other engines provide implementation ideas, not evidence of Taikyoku speed or strength. The recommendations below distinguish source observations, local screening evidence and proposals; the main report supplies final confirmation. External implementation links use the exact revisions recorded in `primary-sources.json`.

## Findings from engine research

**Keep exactness and whole-search measurement as acceptance requirements.** Full routes, scores, total visited nodes, q-node subset, completed depth, static scores and ordered root lines must agree for mechanical changes. Small subsystem gains are insufficient when maintenance, cloning, cache initialization or allocation moves elsewhere. Fixed-time strength requires separate game/tactical evaluation.

**Correct the non-power-of-two TT set-count bug, validated by the final correctness suite.** Widths 3/5/6/7 previously applied a bit mask to a non-power-of-two set count, leaving many allocated sets unreachable. Rounding the set count down preserves widths 1/2/4/8 and keeps every allocated set addressable. This is a correctness/capacity improvement for nondefault Rust configurations; no production speed or Elo gain has been established.

**Credit the bit enumeration already present.** Stockfish's `pop_lsb` obtains the least significant set bit and clears it with `b &= b - 1`. Our board and search candidate iterators already perform the corresponding `trailing_zeros`/clear-bit loop over 21 words. “Use bitboards” is therefore not a new optimization. Further gains must come from reducing words/candidates visited or maintenance cost. [Stockfish bitboard implementation](https://github.com/official-stockfish/Stockfish/blob/ff7cee4244ec96ea488c402669e3674c0574dce6/src/bitboard.h)

## Promising, prioritized experiments

**1. Census remaining attack-filter costs before another index.** A direction-only prototype was implemented and rejected: it preserved near/global/uncertain origins and accepted-piece order, but increased CPU for all three screened agents. Its per-board payload was 3,024 bytes: 2,688 bytes for eight direction masks per color plus 336 bytes for uncertain-origin masks. The whole-search comparison includes ongoing mutation, clone and query costs; it does not isolate each cost. Count actual queries after memo hits, candidates visited before early success, rejected directions and short-distance classes, and place/remove/move/clone work before designing another index. Only proceed if rejected work survives the memo layer and can plausibly repay maintenance.

A future short-class or capture-generation index remains unimplemented and unmeasured. It must over-approximate the existing filter contract: preserve near-range alignment behavior, White's direction transform, base-origin Whale/Rain Dragon distinctions, long short movers and global exceptions. Capture generation requires separate Peacock parity coverage; the rejected direction-only prototype did not narrow that path. Keep VirtualBoard overlay scanning initially. Require class-superset and mutation/clone/make-unmake checks, then exact paired search signatures. Both the older line-occupancy result and the new direction-only result caution against candidate-count projections that omit update costs. The historical design memo below identifies the remaining census and invariants.

**2. Try stable lazy move selection.** Stockfish scores each generated move before ordering, uses contiguous move/score records, and partially sorts selected stages. Its `MoveList` owns a fixed array passed to generation. Ethereal instead keeps parallel moves/values and selects the next highest value on demand. An adaptation could avoid fully sorting tails never reached after a cutoff. Freeze keys before recursion, retain original-index tie breaks and reproduce existing TT-swap semantics; otherwise ordering/history/LMR changes become policy experiments. Direct append into caller-owned growable storage was the smaller first step and is retained in the independently confirmed exact bundle; that confirmation does not isolate its individual contribution. Lazy selection remains an unimplemented proposal. [Stockfish picker](https://github.com/official-stockfish/Stockfish/blob/ff7cee4244ec96ea488c402669e3674c0574dce6/src/movepick.cpp), [move generation buffers](https://github.com/official-stockfish/Stockfish/blob/ff7cee4244ec96ea488c402669e3674c0574dce6/src/movegen.h), [Ethereal picker](https://github.com/AndyGrant/Ethereal/blob/0e47e9b67f345c75eb965d9fb3e2493b6a11d09a/src/movepicker.c)

**3. Evaluate relative quiet history against the existing History arm.** The relative-history paper divides successful/best-move counts by exploration frequency, addressing useful but rarely legal moves. Its LOA benefit was small at shallow depths and grew at deeper searches, so the paper's reported savings are not a prediction for our depth-1–3 workload. Our History arm already has bounded rewards/maluses and capture history, but no explicit attempt denominator. First count conflicting updates caused by endpoint-only quiet identities and FreeEagle route aliasing; then test sparse side/promotion/route-aware statistics with smoothed denominators. Actual AB attempts should drive the denominator rather than selectively admitted q moves. [Relative History Heuristic, author-hosted paper](https://erikvanderwerf.tengen.nl/pubdown/relhis.pdf)

**4. Explore cost-aware ordering as a strength experiment.** Furtak and Buro derive an expected subtree-cost/cut-probability ordering criterion under simplified search assumptions. A Taikyoku experiment could log child work and cut outcomes, then combine cheap cost features—compound-route fanout, capture/update size and recent subtree work—with cut likelihood at non-PV nodes. Measure overhead, tactical misses and fixed-time playing results. Current LMR depends on list position, so changing priority can change which moves receive depth; fewer nodes cannot establish better play. [Minimum Proof Graphs and Fastest-Cut-First Search Heuristics](https://ijcai.org/proceedings/2009/89)

**5. Consider exact TT layout work only after profiling misses.** The pinned Stockfish source packs three entries and partial key tags into a 32-byte cluster, using narrow depth/value fields. The transferable idea is locality. A possible exact adaptation would keep full 64-bit identity and full-range scores/depths, perhaps separating compact probe tags from guarded payloads. Additional loads and tags can erase the benefit; measure cache behavior and complete search. [Stockfish TT implementation](https://github.com/official-stockfish/Stockfish/blob/ff7cee4244ec96ea488c402669e3674c0574dce6/src/tt.cpp)

## Rejected or held aside

**Do not directly transplant chess-sized fixed stack arrays or compact move encodings.** Taikyoku has 1,296 squares and compound routes. Fairy-Stockfish itself increases move-list bounds for variants and explicitly chooses heap storage to control stack growth. Keep a safe growable/spill path unless a complete route bound is proved. [Fairy-Stockfish variant limits](https://github.com/fairy-stockfish/Fairy-Stockfish/blob/53c1f2bd14a2d3b685904ccf8ec9dec8401800ff/src/types.h), [bit enumeration](https://github.com/fairy-stockfish/Fairy-Stockfish/blob/53c1f2bd14a2d3b685904ccf8ec9dec8401800ff/src/bitboard.h)

**Do not directly transplant shortened TT keys, narrow numeric ranges or racy updates.** They alter collision, range or concurrency behavior rather than providing an exact representation change. No such tradeoff is justified by this local sweep.

**Keep TT-first staging and retained search bounds outside the exact-speed bundle.** YaneuraOu explicitly discusses repetition problems from unconditional qsearch TT-first moves. Our previous prototypes also changed search history or choices. Full game/evaluator/config/history ownership and strength validation are necessary before production. [YaneuraOu picker and repetition discussion](https://github.com/yaneurao/YaneuraOu/blob/c6a4a08df662ab3cbb4318b852ca2bcb786c66e8/source/movepick.cpp)

**The four-change search package did not establish a speed gain.** Its combined screen was mixed across models and approximately neutral overall. Individual ablations were then run: indexed root lookup was effectively neutral, stable q retention was mixed, and cached capture scores did not improve complete search. Those three changes were excluded. The precomputed TT mask was timing-neutral and retained as support for the separately validated odd-width capacity fix. The consolidated attack-filter rewrite was also excluded. Standalone Range counting was neutral overall, but its visitor is part of the retained direct-append package; confirmation establishes that bundle rather than an isolated Range-count effect.

No experiment above establishes Elo improvement. Exact fixed-depth speedups can permit different decisions under a clock; changed ordering can also alter selective-search errors. Confirmation should use held-out games, all relevant model families, even arm-balanced repetitions, exact signatures, game-cluster uncertainty, and separately reported tactical or playing-strength evidence.


## Newer agent results found during this sweep

The previous reports are not the latest evidence. The small, hashed manifests and summaries in [search-prior-evidence.json](../search-prior-evidence.json) establish that the existing bounded-History agents have completed games; they do not establish that a new relative-history or cost-aware variant works. No claim about a currently running process follows from saved receipts.

An October 30 Bradley–Terry snapshot over 5,621 completed games assigns descriptive rating points of 1,519.62 to W512_v3 and 1,559.93 to W512_v3_H. A selected same-start, color-swapped pair has two H wins, from source `dcb54bd31db8e5b1693ac9e39e4fab76e0421106`. The export deliberately selected higher-rated eligible opponents and gives no uncertainty interval or total H-specific game count. These are not evidence of a causal 40-point improvement. Those games also use preserved legacy q/progress behavior.

A separate selected HLF export has nine wins and three losses over six color-swapped pairs. HLF combines History, tactical LMR and QFIX, so it cannot isolate History. The one continuation-history comparison is only one selected pair. These results justify comparing a new proposal against existing History policies, rather than recommending bounded history as if it were absent.

The October 31 generation-three screen supplies 18 highest-shared-depth comparisons per trial against ContextRetry on the same HLF/HLFS parent. It used nine convenience positions and one sequential 750 ms capped request per arm/case, not repeated timing or randomized strength testing. QHistory has median cumulative-node ratio 1.000 (range 0.98349–1.00838), with no differing move/score pairs; it reuses capture history in q ordering and is not relative-history normalization. Internal PVS has median 0.99796 (range 0.78504–1.07319), also with no differing move/score pairs. MajorThreatLMR has median 1.03345 (range 0.73974–2.72461), with ten differing move/score pairs. Fewer nodes or changed decisions do not establish stronger play. GrowthAware's clock mechanism is inactive in those fixed-time requests, so its neutral screen cannot adjudicate Fischer clock management.

The recorded iteration-cost analysis is a stronger reason to prioritize time management: among 12,022 corrected-build HLF/HLFS nonmate turns, 3,381 contain an incomplete iteration; incomplete work accounts for 35.86% of summed recorded iteration time. That is **not** a measured recoverable speedup. Existing GrowthAware/partial-iteration ideas should be tested with realistic clocks and decision-quality checks before inventing another heuristic. A later completed PVS pair has one win and one loss, but both sides already use PVS/History, so it cannot measure their individual benefit.

This audit read small existing summaries/manifests and bounded record headers. It ran no new game searches, rating fit or bulk-game analysis. Exact file hashes, dates, worker/engine identities and selection limitations are in the linked provenance record. User analysis and backups were not modified.


## Direction-index experiment outcome



The exact direction-only prototype was tested and rejected: CPU ratio 1.05660 in the common 36-pair screen. All three agents were slower. Its complete maintenance cost is more useful evidence than a candidate-count projection. The earlier proposed census remains a prerequisite before another structural index, particularly on the capture-generation path this prototype did not change.



# Historical design memo: movement-class attack candidate index

This memo began as a read-only proposal. The direction-only first stage below
was subsequently implemented, checked for exact behavior and screened in full
search; it was slower for all three tested agents and was rejected. Its added
per-board payload was 3,024 bytes: 2,688 bytes of direction masks plus 336 bytes
of uncertain-origin masks. The measured comparison includes ongoing mutation,
clone and query costs but does not isolate their individual contributions.

The optional short-class and capture-generation stages remain unimplemented
and unmeasured. The remaining census is a prerequisite before another index;
candidate-count projections alone are not speed estimates.

## Current baseline and the useful distinction

`Board` maintains color occupancy and rare global-attacker masks, each keyed by
square. `candidate_square_masks[target]` is the union of the radius-five window
and eight distant rays. Queries enumerate
`(region & occupied[color]) | global[color]`, then run the per-piece target
filter and actual reach probe. Search capture generation converts those squares
to a slot bitset so it can retain army order.

The proposed index would reject impossible *movement classes* before loading
pieces; it would not maintain attacks or resolve blockers. It is therefore
different from the older rank/file/diagonal line-word prototype, which sped up
path checks but added mutation/clone work and was neutral to slower in its first
integrated run (`ENGINE_SPEED_ANALYSIS.md`, “Set aside” and A1). The subsequent
direction-only result reinforces the need to count update cost; neither result
adjudicates the untested short-class or capture-generation extensions.

## Implemented direction-only first stage

The tested prototype used per-color, per-absolute-direction square masks for pieces with Range
movement toward that direction. For distant origins, intersect each target ray
with the corresponding direction mask, reversing target-to-origin direction to
obtain origin-to-target direction. Keep the near window and existing global
mask unchanged. It also kept uncertain/base-origin fallback masks. It narrowed
board attack queries; capture generation retained its existing candidate path.
The exact prototype lost in the integrated screen and is not retained.

## Untested extensions

Only add short-reach classes if the census shows substantial near-window waste:

- Group ordinary `ReachClass::Short(max)` pieces by the distinct radii actually
  present; intersect with the target's Chebyshev-radius mask. The current filter
  deliberately ignores directional details for this class. Ordinary same-square
  origins are excluded. Do not assume all short movers fit radius five.
- For `ReachClass::Range`, near acceptance requires alignment but ignores the
  piece's Range direction bits inside the window. Use near-ray masks there;
  applying directional Range masks to every near origin would be unsound.
- Keep a near-window fallback class for ordinary `ReachClass::Always` and
  uncertain/base-rewritten forms. Preserve the existing global-mover path for
  Tengu/Peacock/Hook Mover families, Lion Hawk, Cannon Soldier and long short
  movers. Specialized probes still decide actual attacks.
- An optional capturing-only subset can suppress distant check-mode candidates,
  matching the current filter. Add this only if the census finds frequent
  rejections that survive memo hits and occur before early query success.

The index must over-approximate the **existing filter contract**, including its
quirks, rather than derive a new attack definition from movement generation.
White's direction transform differs between attack filtering and generation;
index construction must use the attack transform. Keep the actual reach probes.
Do not reuse a narrower index as a capture-generation guard without separate
parity coverage: the known direct-diagonal Peacock exception remains relevant.

## State and update invariants

Every indexed bit must mean that the square currently contains a piece of that
color and class. Membership depends on full movement identity: piece type,
color, promotion state, and base origin where relevant. In particular,
Reverse-Chariot-origin Whale and Earth-Dragon-origin Rain Dragon must not share
incorrect cached classifications. Unknown variants use the conservative path.

`place_piece` must remove old membership before replacement; `remove_piece`
clears every membership, including friendly sweep victims. `move_piece` transfers
the mover's unchanged membership after destination removal. Its same-square
no-op changes nothing. Promotion currently removes/replaces a piece, so it must
reclassify. Undo restores original identity and all captured identities through
these same mutators. Null moves change no masks. Maintain invariants after every
mutator, including partial multi-leg operations and failure paths.

Square-keyed membership is unaffected by army `swap_remove`, although slot maps
must remain coherent. Clone must copy all masks independently. Preserve ascending
square order in board queries and existing slot order in capture generation;
union overlapping classes before enumeration to avoid duplicates. Existing
global special cases may accept the origin itself, unlike ordinary classes.
Keep VirtualBoard's overlay scan initially; using a base-board index there would
require applying removals, mover relocation and promotion-class changes.

## Minimal census before another index

Collect untimed counters from representative opening/middle/late and long-history
roots, handcrafted and NNUE agents, split by caller and check mode:

1. Actual board queries **after memo hits**, candidate counts, candidates really
   visited before early success, global versus local/ray visits, and rejection
   reasons: short distance, range misalignment, wrong distant direction and
   capturing-only check. Record actual reach-probe successes and misses by class.
2. Shadow candidate counts for direction-only and short-class designs; distinguish
   full-mask reductions from work removed before the original first successful
   attacker. A query that already stops early may gain nothing from rejecting
   candidates it never visited.
3. Actual place/remove/move/clone counts and changed pieces per search, including
   friendly/enemy sweeps and undo. Count directional memberships transferred per
   piece and prospective short/fallback writes. Index maintenance is paid even
   when all attack questions hit memos.
4. Catalogue distinct live radii, special/base-origin class populations and index
   payload/clone bytes. Avoid one dense mask per piece type unless the census
   justifies the extra state.

Proceed only if frequent pre-success rejections survive the memo layer and
outnumber the added maintenance sufficiently to justify an integrated trial.
Require exact candidate-superset tests for every type/color/promotion/base form,
mutation/clone/make-unmake invariants, and paired full-search signature parity.
Subsystem timings alone cannot establish whole-search value.
