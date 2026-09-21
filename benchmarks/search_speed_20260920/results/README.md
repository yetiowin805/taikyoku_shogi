# Search speed experiment results

Selected candidate: `iter+qcache-late+tt-dirty`. Baseline: `aee552ee96416f3b6d5471e8f2498a64742eec03`.

## Confirmation

Primary result, eight held-out positions and four agents: **1.157× speedup (95% position-cluster bootstrap interval 1.087–1.239×; exact two-sided sign-flip p=0.00781)**. This corresponds to **13.6% less elapsed search time**.

The held-out subset contains 512 randomized pairs (1024 timed searches). The complete confirmation contains 768 pairs across twelve positions. Four positions were used for tuning; eight were kept out of candidate selection. Every position/agent received sixteen pairs. Both processes were restarted and rewarmed after eight repetitions.

Handcrafted family (secondary): 1.207× speedup (95% position-cluster bootstrap interval 1.110–1.314×; exact two-sided sign-flip p=0.00781).
NNUE family (secondary): 1.109× speedup (95% position-cluster bootstrap interval 1.056–1.177×; exact two-sided sign-flip p=0.00781).

Per-agent held-out results (secondary; p-values are not multiplicity adjusted):

- `BASE_C2S2_A40_Lmate`: 1.213× speedup (95% position-cluster bootstrap interval 1.105–1.342×; exact two-sided sign-flip p=0.00781).
- `C2K50A1`: 1.201× speedup (95% position-cluster bootstrap interval 1.113–1.302×; exact two-sided sign-flip p=0.00781).
- `NNUE512`: 1.126× speedup (95% position-cluster bootstrap interval 1.057–1.214×; exact two-sided sign-flip p=0.00781).
- `NNUE2048`: 1.092× speedup (95% position-cluster bootstrap interval 1.046–1.148×; exact two-sided sign-flip p=0.00781).

The position-level mean improved on all eight held-out games, ranging from 1.031× to 1.395×. The largest gain was in champs-midgame; late-royal and late-endgame gained about 3%. These differences show why a single opening benchmark would be inadequate.

## What worked and what did not

- Allocation-free direction/ray iteration removed temporary vectors while preserving enumeration order and every path square.
- Quiescence ordering now counts enemy royals once after filtering, derives last-royal capture status from already-computed capture metadata, and performs no such work for a singleton list. The original comparator repeatedly scanned the opposing army.
- Table storage is reused but all written slots are reset after each search. No transposition bounds survive into another search or model. Fully clearing the entire table was slower in tuning; clearing written slots improved the result.
- Thread-local move buffers were slower in the first pass. Moving the pool into the search context and varying initial reservation between 0 and 256 moves also reduced the combined gain. These changes were excluded.
- The handcrafted evaluator benefited more than NNUE. That is consistent with NNUE inference consuming a larger share of total time; these changes optimize shared search work, not the neural-network arithmetic. This interpretation is not an exclusive-time profiling measurement.

## Validity and limits

- 2,368 timed searches across screening, refinement and confirmation, including an identical-binary A/A control. The control measured approximately 1.005×, much smaller than the selected effect.
- All 768 confirmation pairs matched full best routes, scores, main/q node counts, static scores, completed depths, and ordered root-line fingerprints. Chosen routes were checked against the complete legal move list.
- Held-out speedup by independent process block: first eight repetitions 1.154×; second eight 1.160×.
- CPU-time cross-check on held-out positions: 1.157× speedup (95% position-cluster bootstrap interval 1.086–1.238×; exact two-sided sign-flip p=0.00781).
- Fixed depth ceilings were chosen using stock-only one-second calibration, capped at depth 3. Comparisons used no deadline and identical depth/settings within each pair. Models and full game histories were loaded outside search timing; fresh-search setup and TT cleanup remained inside timing.
- Timing order was randomized and balanced on pinned CPU 2. No builds ran during measurement. Confidence intervals resample positions, not individual repeated timings; the exact sign-flip test likewise uses position clusters.
- The primary statistical test was specified before confirmation and uses only held-out positions. The overall/tuning and per-agent/family summaries are secondary. Twelve chosen positions on one Intel i7-1255U are not a random sample of every possible game or CPU.
- This is evidence of faster equivalent fixed-depth search, not an Elo result or a guarantee of identical moves under a time limit. The tested evaluators were two handcrafted checkpoints and NNUE widths 512 and 2048; intermediate NNUE widths were not tested.
- Reusable TT buffers retain allocations between searches. Resident-memory deltas are recorded in experiment.json; allocator behavior means retained allocation size is not the same as incremental RSS.
- Observed median candidate-minus-stock resident memory was 6.6 MiB across confirmation pairs. Cold-start, densely filled tables at greater depth, and concurrent tournament workers were not measured.
- Correctness: 366 library tests passed in both debug and release, with four existing ignored tests in each profile. Three analysis-method tests passed. New coverage compares all 1,679,616 square pairs and all 256 direction masks with the previous APIs and checks TT clearing across cluster widths.

## Reproduction and evidence

See the parent README for commands and protocol. This directory contains compressed raw paired observations, all phase summaries, input identities, compiler/binary/source provenance, selection details, test logs, and the measured source patch. Game records and trained NNUE blobs remain in the ignored local output directory; inputs.json records their original locations and hashes.
