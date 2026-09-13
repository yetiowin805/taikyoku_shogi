# Fast search experiment screen — 13 September 2026

All 13 experimental variants were implemented independently behind `search-experiments` and measured against the combined mechanical build. They are disabled in normal builds. Nothing was merged or deployed; the live VPS tournament was untouched.

The strongest follow-up candidates are **swap-removal** and **aspiration half-width 500**. Half-width 100 is also worth keeping as a comparison. These results justify more targeted checks, not enabling a search-changing policy in the tournament.

## Scope and controls

487 search invocations took about **6 minutes 36 seconds** of process wall time in total, excluding builds and correctness tests. Each invocation ran sequentially on local logical CPU 0, with no intentional concurrent compilation, testing, or benchmark workload. The CPU was not reserved against unrelated OS/user activity.

- Frozen six-position screen: dense opening, wide promotion root, multi-leg captures, last-royal evasion, a 1,031-ply history, and a finite saved-score sign crossing. Original checkpoints and complete game prefixes are copied under `inputs/`; the opening record represents the standard initial position.
- Baseline check: unchanged main `9d3058696779ed1664c54e336d351c1ef006acdf`, normal mechanical build, and experimental build with only mechanical changes; two shuffled depth-2 blocks per position/build.
- Each experimental variant: two shuffled depth-2 blocks over six positions, two one-second searches per position, and one depth-3 follow-up on five positions. The depth-3 subset was selected solely by the mechanical baseline finishing within two seconds; the wide promotion position did not qualify. Each fixed-depth process had a 20-second external limit; **no run failed or hit it**.
- Cross-turn follow-up: two repetitions on five game positions, warming the same agent/model two plies earlier in the same process, then searching the target with a depth-3 ceiling and one-second budget. Baseline received the same warm-up. No intervening opponent search was inserted: this tests compatible reuse, not the complete alternating-player tournament pipeline. The prototype retains only the most recent model/policy identity and rejects mismatches.
- Nine additional fixed-depth searches independently checked the memory regression in the wide root, with and without reusable storage.
- Full debug and release correctness suites with the feature enabled: **345 passed, 4 intentionally ignored**, in each profile. Targeted new tests cover mobility invalidation/clone/unmake, TT identity/context rejection, and exact restricted TT-first route/order/node parity.

The harness excludes parse, replay and metadata preparation from search timing, and includes ordinary search allocation/cleanup. Cold experiments use a fresh process; cross-turn measurements deliberately include retained search state. Peak RSS is process high-water memory, not a count of allocations. Metrics do not include profiling instrumentation.

Times below are **percentage of elapsed time saved**, relative to the mechanical baseline in the same repetition block. Positive is less time; negative is more time. For each position take the median of its paired ratios, then geometrically average the position ratios. Total-time savings instead compare sums of per-position median elapsed times. With two repetitions (one at depth 3), small differences are inconclusive. No confidence intervals or playing-strength estimates are claimed.

## Main versus mechanical baseline

The normal mechanical build saved **9.8%** by equal-position weighting and **6.7%** by total elapsed time. The experimental mechanical control saved 9.9% / 8.4%. All baseline comparisons preserved completed depth, total and q-node counts, static/search scores, complete chosen routes, and ordered root lines.

**The mechanical bundle is not yet ready to merge wholesale.** Reusable storage raised the wide position's peak RSS from about **33.7 MiB to 42.4 MiB (+26%)**. A second three-repetition check reproduced this; disabling storage alone returned RSS to 33.7 MiB, while retaining almost all the timing improvement in that position (median 540.5 ms without storage versus 537.8 ms with it; stock 595.4 ms). Hold the current storage implementation out of the eventual production selection until its retained capacities are bounded or the regression is otherwise resolved. This screen uses the same complete mechanical baseline consistently for every experimental comparison.

## Experimental results

- **Dense lists with swap-removal — promising follow-up candidate.** Depth 2: **7.4%** time saved; depth 3: **4.7%** (total-time savings 7.5% / 4.2%). Same chosen move and final score in every measured fixed-depth position, but root lines and node counts changed. One-second searches visited 3.4% more nodes on average, with no additional completed depth. This is a search-changing variant because removals change iteration order; the sampled unchanged best moves do not make it mechanical.
- **Incremental mobility — not promising in this implementation.** Depth 2: **−36.6%**; depth 3: **−45.7%**. Exact fixed-depth results and nodes matched. One-second searches visited 21.8% fewer nodes and completed a shallower iteration in 4/12 pairs. The cache invalidates every geometrically affected first-leg ray on every board mutation, including intermediate capture, promotion and undo. That conservative invalidation and cache copying cost more than the avoided counting. This rejects this cache design, not every possible incremental mobility design.
- **Restricted TT-first generation — inconclusive.** Depth 2: **1.2%**; depth 3: **0.4%**. Results, root lines and nodes matched. Only 14 eligible depth-3 probes occurred, with eight early cutoffs. Preserving subsequent generation order requires copying the original position and ordering heuristics before the first search; that cost limits the saving. Ambiguous routes and enabled sibling reduction modes fall back to ordinary generation.
- **TT move promoted ahead of its stage — inconclusive.** Depth 2: **1.4%**; depth 3: **0.3%**. No changed chosen moves in this sample. There were only 15 depth-3 probes, so this sample barely exercises the additional eligibility. The variant can move a quiet multi-leg TT route before Stage A and is search-changing.
- **Interior PVS — not promising in this implementation.** Depth 2: **1.6%**; depth 3: **−37.7%** (41.5% more total elapsed time). One of five fixed-depth chosen moves changed; one-second searches changed the move in 2/12 pairs and completed a shallower iteration in 4/12. The depth-3 sample performed 30,511 narrow probes and 601 re-searches. Probe/re-search interactions with the existing selective search increased work despite the modest re-search fraction.
- **Aspiration half-width 100 — mixed, worth retaining for follow-up.** Depth 2: **6.4%**; depth 3: **2.0%**. Total-time savings were only 1.8% at depth 2 and **−1.7%** at depth 3. Four of 12 depth-2 scores changed, but chosen moves did not. There were 14 widening retries across the five depth-3 positions. One-second searches visited 9.8% more nodes; none completed a deeper iteration than the baseline.
- **Aspiration half-width 500 — promising follow-up candidate.** Depth 2: **6.1%**; depth 3: **7.2%**. Total-time savings were 0.4% / 3.3%. Four of 12 depth-2 scores changed, with unchanged chosen moves. Six depth-3 widening retries occurred. One-second node gain was 8.7%, with no additional completed depth. Gains concentrate in the multi-leg and long-history positions; the finite-reversal depth-3 position was 6.3% slower. Keep the model/position dependence visible in a follow-up.
- **Aspiration half-width 2,000 — no useful evidence yet.** Depth 2: **−1.8%**; depth 3: **−0.8%**. No depth-3 widening retries occurred. Two of 12 depth-2 scores changed, with unchanged chosen moves. The wider window provides little reduction in this screen.
- **Cross-turn TT move hints — inconclusive.** Cold depth 2: **0.5%**; cold depth 3: **−1.9%**, which mostly measures bookkeeping. In the actual compatible warm-up test, 328 hints were accepted; equal-position elapsed time was **0.3% slower**, with unchanged scores, chosen moves and completed depths. This implementation accepts a retained hint only when it resolves to exactly one full legal route. A separate cache/binding strategy would be needed to retain both players' models through alternating tournament searches.
- **Cross-turn TT bounds — not promising under the current conservative design.** Cold depth 2: **−28.0%**; cold depth 3: **−43.8%**. Warm-up test: **20.4% more elapsed time** by equal-position weighting, two shallower completed depths out of ten pairs, 634 context mismatches and **zero accepted score/bound entries**; 328 hints remained usable. Context fingerprints include model/policy identity, full repetition history, draw state, board traversal, incremental evaluation state, ply, windows, ordering heuristics, null/PV eligibility and q-entry context. Unsafe or unmatched score reuse is rejected. Computing this context is costly and this screen found no valid bound reuse to offset it.
- **Deadline checks every 8 nodes — inconclusive.** Depth 2: **1.6%**; depth 3: **−1.6%**. One-second node change: **−0.2%**. Maximum observed whole-search-call overshoot: **0.8 ms**.
- **Deadline checks every 32 nodes — inconclusive.** Depth 2: **1.0%**; depth 3: **−2.6%**. One-second node change: **0.3%**; one pair completed a shallower depth. Maximum observed overshoot: **1.8 ms**.
- **Deadline checks every 128 nodes — inconclusive.** Depth 2: **1.6%**; depth 3: **−1.4%**. One-second node change: **0.8%**. Maximum observed overshoot: **1.5 ms**. All throttled variants retain checks around expensive generation/evaluation, and the analyzer watchdog is unchanged. These short runs do not bound worst-case overshoot.

None of the experimental variants increased peak RSS by more than about 2.1% relative to the shared mechanical baseline in these runs. That does not remove the independent mechanical storage regression described above.

## Representative changed outcomes

Scores here are **side-to-move relative**, as returned by search. They are not win probabilities or evidence that the higher-scoring variant plays better. TM1 coordinates are engine-native, zero based; raw JSON includes every intermediate square.

- **PVS, dense opening, depth 3:** baseline chose `17,9-14,12`, score **14**, 22,990 nodes, 548.1 ms. PVS chose `24,9-24,11`, score **34**, 36,296 nodes, 821.9 ms. This is the one changed chosen route in the depth-3 screen.
- **Aspiration 100/500, finite reversal in slot0046 after 19 recorded moves, depth 2:** the same move `17,32-3,18` was selected, but its reported score changed **1,280 → 2,680**. The wider 2,000 window retained the original score here. This is a meaningful evaluation difference and requires tactical investigation before adoption.
- **Aspiration 100/500/2,000, wide promotion root in slot0569 after 187 moves, depth 2:** the complete route `26,17-35,26-34,27` stayed the same, with score **14,598 → 14,596**.
- **Aspiration 500, slot0670 multi-leg position, depth 3:** score **17,008** and chosen route stayed the same; elapsed time fell **906.7 → 751.6 ms**, with **43,941 → 37,847 nodes**.
- **Aspiration 500, slot1233 after 1,031 moves, depth 3:** elapsed time fell **153.5 → 120.9 ms** with unchanged chosen move and score. This cheap position contributes more to the equal-position percentage than to the total elapsed-time percentage.

## Limits and next decisions

This is an initial screen, not the earlier full 24-position acceptance matrix. Each board group has only one representative; sibling modes 1–4, all q-policy combinations, mate-transition/terminal coverage, longer deadlines, allocations and analyzer end-to-end recovery/throughput remain outside this screen. The batch analyzer code from the larger work item is not validated by these search measurements. No tournament strength claims follow from the NPS or depth counters.

A focused next comparison should keep swap-removal and aspiration 500, include aspiration 100 as a comparison, and inspect the finite-reversal score discrepancy. Address the mechanical storage memory issue before preparing a production merge. The other experimental implementations should stay disabled while awaiting either a better design or evidence from a specifically relevant position.

## Reproduce and inspect

- `corpus.json`: frozen input selection; `ply` means the number of recorded array entries applied before search.
- `inputs/`: immutable full game records and original checkpoint contents.
- `provenance.json`: baseline revision, per-input and source hashes, binary hashes, compiler settings and CPU details.
- `tested-source.patch`: complete source/harness patch relative to the baseline revision, including newly added Rust files.
- `raw.jsonl.gz`: all 487 outputs, including full chosen routes, ordered root lines, timings, RSS, completed depth, nodes, q-nodes, experimental counters and failures.
- `results.json`: per-position and aggregate comparisons, including changed-move details and the independent memory check.
- `../run_pilot.py` and `../summarize_pilot.py`: orchestration and aggregation. The runner appends results; use a fresh output archive for a new experiment batch.

Build the unchanged base with the same `examples/search_bench.rs` harness and place its release binary at `/tmp/taikyoku-bench-bins/stock`. Build this source normally for `/tmp/taikyoku-bench-bins/production`, then with `--features search-experiments` for `/tmp/taikyoku-bench-bins/experiments`. Build all binaries before timing.

```bash
cargo build --offline --release --example search_bench
cargo build --offline --release --features search-experiments --example search_bench
python benchmarks/search/run_pilot.py baseline --cpu 0
python benchmarks/search/run_pilot.py screen --cpu 0
python benchmarks/search/summarize_pilot.py
```

For a single experimental search, the interface is `search_bench CORPUS INDEX DEPTH TIME_MS VARIANT [REPEATS] [previous]`. `TIME_MS=0` uses the original model's unlimited default in these frozen checkpoints. Named variants are listed in the runner; an integer instead selects the mechanical bit mask (`511` all, `507` without storage). All experimental features remain off for normal builds.

## Subsequent production default

The VPS follow-up disables reusable storage by default (`507`). The pilot above used `511`; its results remain unchanged historical measurements. The `storage` experiment explicitly enables the full old bundle.

## Data availability

Full game records, checkpoint copies and detailed raw results are deliberately excluded from Git. They remain in the originating local benchmark directories and, for the follow-up, `/opt/search-screen-20260913/` on the VPS. The checked-in corpus paths and SHA-256 identities identify the required inputs; copy the preserved inputs into the corresponding `inputs/` directory before reproducing.
