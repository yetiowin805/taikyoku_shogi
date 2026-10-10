# W512 v4.5 search-efficiency experiments

This is an exploratory implementation and measurement bundle, not a new playing-strength release. It tests behavior-preserving quiescence preparation, cached quiet multi-leg ordering, and guarded interior principal-variation search (PVS). The implementations require the `search-study` Cargo feature and explicit per-search switches. Normal builds retain the existing search behavior.

## Decision

Keep the prototypes available, but do not change the agent defaults on this evidence. Lazy royal-capture checking remains a plausible small optimization. The combined q preparation changes and cached stage-B ordering do not have an established benefit. Interior PVS deserves a more careful playing-strength comparison only if its allocation changes translate into better decisions; neither fewer nodes nor a different move is sufficient evidence by itself.

The experiment uses the actual **W512 v4.5** network throughout performance measurements. The harness fails if NNUE is missing, validates each chosen full move route, preserves the complete game prefix, and excludes replay/model loading/warmup from the measured search. Synthetic unit tests exercise search contracts independently of NNUE. See [harness](../../examples/nnue_efficiency_bench.rs), [protocol](PROTOCOL.md), and [identities](manifest.json).

## Completed fixed-depth evidence

- **Lazy royal probing:** 36 paired depth-4 runs over 12 positions. Median-per-position geometric mean CPU ratio **0.9870** (about 1.3% less CPU); exploratory game-cluster bootstrap interval **0.9542–1.0109**. All 36 search signatures match exactly. The candidate skipped **925,497** royal-capture queries across those runs.
- **Combined q changes:** the same 36 pairs match exactly, with CPU ratio **1.0110**, interval **0.9876–1.0359**. Early filtering rejected **244,689 of 11,197,953** raw candidates (2.19%) before detailed annotation. This avoids some work, but does not demonstrate a net speedup.
- **Cached stage-B sort keys:** all 36 pairs match exactly; CPU ratio **0.9965**, interval **0.9782–1.0166**. No established gain.

These ratios use CPU time, not nodes or wall time. The signature includes chosen full route, score, completed depth, total/q nodes, q generated/searched counts, q TT hits and reported root lines. The intervals resample games rather than treating repeats as independent positions; they do not account for every systematic timing error. The control's median per-position max/min CPU ratio across its three repeats is **1.084**. There was also a later power-state incident, with an unknown transition time. Small earlier timing differences therefore remain provisional. Backing: [confirmation rows](results/confirm.jsonl), [frozen plan](results/confirm-plan.json), [analysis code](analyze.py), [summary](results/summary.json), [power-state account](PROTOCOL.md#power-state-incident-and-exclusions).

The initial depth-3 screen is retained for provenance, not promoted over the repeated test. Both its V1 control and a final default-off V3 check matched unmodified-main search signatures on **12/12** cases. Backing: [baseline rows](results/identity.jsonl), [screen rows](results/screen.jsonl), [final V3 rows](results/identity_v3.jsonl), [identity analysis](results/summary.json).

## What guarded PVS changed

The final V3 prototype scouts later interior moves at remaining depth at least 1 (`pvs1`) or 2 (`pvs2`). It restores incoming capture context on re-search, verifies a PV fail-high with the full PV horizon, and avoids using non-PV transposition scores as PV search results. `guard` isolates the last safeguard without adding scouts. This matters because this engine uses q depth 2 for PV leaves and at most 1 for non-PV leaves; a naive PVS conversion changes which tactical horizon supplies the answer. See [leaf policy and PVS implementation](../../src/search.rs), plus the three study tests in that file.

In the single-pass depth-4 comparison over 12 positions, CPU ratios were **0.9823** for PVS1, **0.9755** for PVS2, and **1.0705** for the guard alone. Their exploratory intervals all include 1. Both PVS variants used approximately **3.6% more nodes** by the geometric paired measure. None of the three changed the selected root move in these 12 cases. This is not a robust 2% speedup or evidence of equal strength. Earlier V1/V2 PVS implementations were superseded and are not pooled into this result. Backing: [V3 rows](results/pvs_deep.jsonl), [plan](results/pvs_deep-plan.json), [summary](results/summary.json), [version patches](research-v3.patch).

The existing root scouts already make **38.0%** of positive-depth interior calls narrow-window: **27,721 of 44,719** control calls were wide. These counters include calls that subsequently return through a transposition-table hit; they are not a count of fully expanded nodes. Interior PVS therefore operates on an already selective tree. PVS1 made **908,332** scouts and **1,362** full-window re-searches, including **229** PV fail-high verifications; PVS2 made **447,050**, **560**, and **94**, respectively. These explain the mechanism, not its strength. Backing: [V3 diagnostic counters](results/pvs_deep.jsonl).

## Equal-time result: no demonstrated PVS advantage

The final clock comparison completed **96 searches**: control, PVS1 and PVS2 on 16 positions, twice each, with a five-second hard budget and depth ceiling 8. Each candidate has 32 paired comparisons. **Neither gained a completed ply. Both lost one ply in two comparisons**, which are repetitions of the same position (`slot0787-p1192`): control completed depth 6, both candidates depth 5. Mean completed depth was 3.9375 for control and 3.875 for either candidate. PVS1 changed the chosen move in 4/32 pairs (two distinct positions); PVS2 changed it in 2/32 (one position). These are unadjudicated differences, not identified improvements or blunders. Backing: [timed rows](results/timed.jsonl), [paired summary](results/summary.json), [case-level evidence](results/report-evidence.json).

All three arms repeated the same completed depth and move on all 16 positions. All **192** before/after power readings confirmed AC and `balance_performance`. This makes the observed one-position depth loss more credible than the earlier small timing differences, but does not turn 16 positions into a strength trial. Source: [power and repeat-stability aggregation](results/report-evidence.json), [derivation code](build_evidence.py).

Mean search wall time was **4.091 s** for control, **3.951 s** for PVS1 and **4.067 s** for PVS2. The ordinary next-iteration admission rule remained active, so an engine can stop before five seconds when the last iteration suggests the next cannot fit. Lower elapsed time is not itself a win: PVS1 also completed less depth on the case above. No search reached the ceiling of 8. Do not interpret the empty matched-depth-8 CPU ratio in the summary as missing timed observations. Source: [timed aggregation](results/report-evidence.json) and [iteration admission](../../src/search.rs).

This result lowers the priority of shipping the current PVS package. It does not rule out a different PVS adaptation or better selective search. In particular, this package changes PV TT handling and restores incoming capture context as well as adding scouts; it is not an isolated experiment on window size alone.

## Where quiescence time goes

Six separately instrumented depth-4 searches attribute **9.21%** of total wall time to q move generation, **2.04%** to candidate annotation, **0.24%** to q sorting, and **3.48%** to positive-budget q evaluation, weighted by total search time. Q generation varies from **1.35% to 18.97%** across positions. Even completely eliminating the measured q sort would save only about 0.24% in this workload. The real ceiling for a cached-sort substitution is smaller.

The existing global make/unmake timer accounts for **16.4–29.9%** across the same searches, and global evaluation for **6.7–11.8%**. Those global scopes overlap the local q spans and must not be added to them. NNUE accumulator updates also occur during make/unmake; the evaluation timer is not the total cost attributable to NNUE. Positive-budget q evaluation excludes bare depth-zero evaluation. Instrumentation itself adds overhead. Backing: [profile rows](results/profile.jsonl), [instrumented plan](results/profile-plan.json), [profiling source](../../src/profile_timers.rs).

This does not show that search is close to optimal. It shows that several inexpensive-looking q preparation changes target small or already cheap components. A different move-generation strategy, reduced make/unmake cost, better pruning decisions, or better ordering before a cutoff has more scope than caching q sort keys. That is a prioritization inference from the profiles, not a measured gain for an unimplemented algorithm.

## Memory and coverage

No prototype adds a model, transposition table, or persistent history table. Cached-key sorting uses temporary storage proportional to the move list; PVS adds recursive re-search work without a new persistent search table. The confirmation process reported a high-water resident set of **298.1–313.9 MiB**. This includes the model and warmed caches and is a monotonically accumulated process high-water mark, so it cannot identify a per-variant memory delta. The study did not compare network widths or claim that W512 is the fastest width. Backing: [source diff](research-v3.patch), [RSS fields](results/confirm.jsonl).

The depth-4 set has 12 positions from 9 games: six population positions, three score-swing event candidates, two known-loss tactical positions, and one earlier validation position. The timed set has 16 positions from 12 games. Every case had already been exposed in the preceding investigation. The corpus labels describe selection, not ground-truth tactical answers; a large recorded evaluation swing is not proof of a forced loss. Full source records and model binaries are local inputs and are not included in this bundle. Backing: [corpus](corpus.json), [request lists](results/confirm-plan.json), [timed plan](results/timed-plan.json).

The interrupted extra q batch preserves **9/56** planned rows. Its three complete fixed-depth pairs match; its timing and deadline-based depth differences are excluded following the power-state incident. It is not an independent validation result. Backing: [raw interrupted rows](results/holdout.jsonl), [power snapshot](results/host-power-snapshot.json), [protocol](PROTOCOL.md#power-state-incident-and-exclusions).

## What would justify adoption

For a behavior-preserving optimization, a reproducible small speedup on deployment-like hardware can suffice; it need not change the best move or add a whole ply in this small sample. For PVS or pruning policy changes, use equal wall-clock budgets and positions selected before observing the candidate, then paired games with starts and colors balanced. Group outcomes by game. A larger tactical suite should retain ordinary positions while enriching rare decisive defenses, route captures and wipes, promotions that create threats, loss of a last royal, and quiet moves whose reply matters around the depth-4/5 horizon.

Use deeper or broader search only to adjudicate disagreements, retain competing candidates in that adjudication, and report unresolved cases. Such a reference remains an imperfect engine judgment, not a solved-game oracle. This round does not estimate Elo or prove that an unchanged move in a sampled position is optimal.

## Reproduction and validation

The base revision is `99f8c9b3d4c4c9fc70e1ee58d362e249310cb5a6`. The final source is feature-gated in this branch; `research-v1.patch`, `research-v2.patch`, and `research-v3.patch` preserve the earlier/final search changes relative to that revision. The common Rust example is also part of the branch. Each frozen plan records binary and corpus hashes. Consult [manifest](manifest.json) before pooling groups from different versions.

Supply the original game records and checkpoint locally, then run from the repository root:

```bash
python3 benchmarks/nnue_search_efficiency/materialize.py --games /path/to/games --model /path/to/NNUE_W512_v4.5/model.json
CARGO_BUILD_JOBS=1 RUSTFLAGS='-C target-cpu=native' cargo build --offline --release --features search-study --example nnue_efficiency_bench
mkdir -p benchmarks/nnue_search_efficiency/bin
cp target/release/examples/nnue_efficiency_bench benchmarks/nnue_search_efficiency/bin/study
python3 benchmarks/nnue_search_efficiency/replay_plan.py benchmarks/nnue_search_efficiency/results/confirm-plan.json --name confirm-replication
python3 benchmarks/nnue_search_efficiency/analyze.py
python3 benchmarks/nnue_search_efficiency/build_evidence.py
```

The fixed named analysis groups describe the archived experiment; analyze a newly named replication separately before combining it. A frozen plan's historical binary path may now point to a newer binary: the recorded hash and manifest identify the version. Profile runs require both `search-study,search-profile`. Pinning CPU 2 is specific to this host and should be changed explicitly for another host. All measured groups use one serial process; do not overlap builds or benchmarks with timing runs.

The V3 library suite passed **433 tests**, with **4 pre-existing ignored tests**, including three new structural checks for q equivalence, PVS re-search, and PV TT handling. All-targets checks passed both with `search-study` and with default features. The default example build has a harmless `unused_mut` warning because only study/profile builds mutate the output JSON. Backing: [full test log](results/all-tests.log), [study check](results/all-targets-check.log), [default check](results/default-build-check.log). The accompanying local interactive report passed its TypeScript check; its opening was queued by the app, so this is not a claim of visual runtime validation. See [report check](results/canvas-check.log).

## External backing

[Stockfish search](https://github.com/official-stockfish/Stockfish/blob/master/src/search.cpp) supplies the precedent for narrow-window scouts, full-window PV re-searches and restricted TT score cutoffs. [Stockfish's move picker](https://github.com/official-stockfish/Stockfish/blob/master/src/movepick.cpp) supplies the precedent for staged move selection and precomputed scores. [YaneuraOu search](https://github.com/yaneurao/YaneuraOu/blob/master/source/engine/yaneuraou-engine/yaneuraou-search.cpp) provides a shogi example of selective alpha-beta and quiescence. These are mechanism references, not evidence for any Taikyoku performance claim above.
