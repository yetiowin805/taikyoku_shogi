# NNUE ablation after the search-overhead merge

The full NNUE bundle still improves the newly merged search baseline: **1.39× throughput at width 512 and 2.34× at width 2048** in this short test. All 48 depth-2 comparisons preserve scores, complete chosen routes, ordered root lines, main node counts and quiescence node counts. Seven targeted release NNUE tests pass. No models, production defaults or live tournament processes changed.

## Baseline and scope

Baseline: main `05b6ec7`, including the merged allocation-free ray/direction iterators, quiescence ordering metadata and TT allocation reuse. The control is the default-feature build of the integrated experiment branch, with NNUE probes compiled out. All six configurations use `-C target-cpu=native`, so these gains exclude any contribution from changing compiler CPU settings. The source merge revision is `5f069c8`; source, patch and binary hashes are retained in the manifests.

Full = cached ability-vector sums + saved accumulator undo + packed byte dot products + skipping neural updates in rule-only royal-safety probes. The other four configurations remove exactly one of those components. Quantizer simplification, residual memoization and head pruning are off. Every checkpoint and its full replay prefix is unchanged.

Four previously used saved positions (opening ply 0, middlegame 160, tactical 360, long-history 2000), trained v2 widths 512/2048, two balanced shuffled three-second repetitions: **96 timed searches**. Each process first runs an untimed depth-1 search to exercise warm TT storage, then measures the configured search. Loading and replay are outside search timing. Runs were sequential on local i7-1255U CPU 0; all builds/tests finished before timing. Depth-2 verification uses one observation per configuration/position/width. This is not a fresh held-out sample or a tournament-strength test.

## Marginal contribution of each component

Each percentage below is full-bundle NPS divided by NPS with that component removed, minus one. Geometric means of paired ratios give every position equal weight. **Contributions are conditional on the other components and must not be added.**

- **Ability-vector cache: +8.2% / +22.7%** at widths 512 / 2048. Individual repetition aggregates: (+9.3% / +7.1%) / (+22.3% / +23.0%). Single depth-2 timing cross-check: +5.2% / +23.3%.
- **Saved accumulator undo: +4.6% / +2.1%** at widths 512 / 2048. Individual repetition aggregates: (+2.5% / +6.8%) / (+3.6% / +0.7%). Single depth-2 timing cross-check: +0.6% / +3.7%.
- **Packed byte dot products: +3.4% / +10.2%** at widths 512 / 2048. Individual repetition aggregates: (+4.9% / +1.9%) / (+13.4% / +7.0%). Single depth-2 timing cross-check: -1.7% / +9.6%.
- **Royal-safety neural bypass: +7.4% / +25.2%** at widths 512 / 2048. Individual repetition aggregates: (+5.6% / +9.2%) / (+27.0% / +23.4%). Single depth-2 timing cross-check: +9.7% / +28.5%.

The cache remains useful at both widths; its benefit is small or absent in the 512-wide tactical case once the royal bypass removes most neural updates. The bypass itself is concentrated in that tactical case: +29% / +129% NPS at widths 512 / 2048 when restored to the other three changes. At width 2048 the full bundle completed depth 3 twice, versus depths 1 and 2 without the bypass. Quieter-position bypass effects are small or noisy; one forcing position drives most of its aggregate gain.

Packed arithmetic has the strongest consistent secondary case at width 2048: each saved position improves roughly 9–11%, with +9.6% in the identical-work depth-2 cross-check. At width 512 its +3.4% timed gain disagrees with the single -1.7% depth-2 timing, so that width remains inconclusive. Saved undo is modest: +4.6% / +2.1% timed, versus +0.6% / +3.7% in the single fixed-depth timings. Keep these small estimates tentative.

## Full bundle against the merged baseline

- **Width 512: 1.386× equal-position NPS; 1.305× pooled NPS.** The two repetition aggregates are 1.373× and 1.398×. Single fixed-depth timing: 1.504×. Median full-process peak RSS increases 7.4 MiB (2.8%).
  - long-history: +28.4% NPS; completed depths 4,4 → 4,4.
  - middlegame: +20.6% NPS; completed depths 3,3 → 3,3.
  - opening: +21.9% NPS; completed depths 3,3 → 3,3.
  - tactical: +95.2% NPS; completed depths 1,1 → 2,2.
  - Timed chosen routes differ in 2/8 pairs. Both changes are in the tactical position, where the full bundle finishes a deeper iteration; fixed-depth routes match.
- **Width 2048: 2.336× equal-position NPS; 2.057× pooled NPS.** The two repetition aggregates are 2.424× and 2.252×. Single fixed-depth timing: 2.401×. Median full-process peak RSS increases 32.0 MiB (3.3%).
  - long-history: +81.2% NPS; completed depths 2,2 → 2,2.
  - middlegame: +82.9% NPS; completed depths 3,3 → 3,3.
  - opening: +66.0% NPS; completed depths 3,3 → 3,3.
  - tactical: +441.4% NPS; completed depths 1,1 → 3,3.
  - Timed chosen routes differ in 2/8 pairs. Both changes are in the tactical position, where the full bundle finishes a deeper iteration; fixed-depth routes match.

The maximum measured search duration was 3.012 seconds (about 11.6 ms beyond the nominal limit, including return/cleanup). All selected moves were legal. Timed NPS can be affected by reaching a different part of the tree, so identical-work fixed-depth results are reported as a secondary check; one fixed-depth observation is not a precise speed estimate. No new claim about evaluation quality or Elo follows from deeper timed searches.

## Recommendation

The newly merged search optimizations do not remove the case for the NNUE bundle. Prioritize the ability-vector cache and royal-safety bypass; packed arithmetic also has a clear case for wider networks. Saved undo remains a small, plausible addition, while packed arithmetic at width 512 needs more samples to estimate accurately. There is no aggregate timed regression in this small study, and measured median memory growth is about 3%, but four selected positions cannot establish universal non-regression.

No larger matrix is warranted for this quick pass. Before production use, retain the existing exactness tests, run the full correctness suite for the final integration, and rebuild/recheck on the VPS CPU. Its instruction set and memory behavior may differ. Intermediate network widths were not retested.

## Reproduction and evidence

See `README.md`, `post_merge_ablation.py` and `post_merge_summary.py`. This directory’s `post-merge-results/` contains compressed complete timed/verification records, manifests with game/model/source/binary hashes and compiler/hardware settings, the summary, predetermined plan, and targeted test log. Large immutable games, model blobs, executables and individual stderr files remain in local `data/nnue-post-merge-ablation-20260920/`. The output writers refuse to overwrite an existing run.
