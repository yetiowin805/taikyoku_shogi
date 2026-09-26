# Low-CPU search optimization screen

Baseline: `61c2875`, including PRs #117 and #118. Work stopped at the user's
return, after the already-running cached-property experiment. No production
optimization, model, or live tournament was changed.

## Findings

**Lazy leaf checks are the strongest completed lead.** Trying cheap sufficient
quiescence-entry conditions before expensive tactical discovery removed about
9.9% of search CPU time (equal-case geometric mean candidate/baseline 0.9011).
Across the four agents, reductions were 11.3%, 8.6%, 11.6%, and 8.1%; across the
six positions, 6.5%–15.7%. Total measured search CPU time fell from 17.252 to
15.647 seconds, a 9.3% reduction. Peak reported process RSS was effectively
unchanged. These are CPU-time savings, not a measured wall-time or strength gain.

The change handles last-royal evasions first and avoids discovering promotions
or other tactical entry conditions once another condition already requires
quiescence. All sampled models use quiescence depth two; the extra zero-depth
shortcut therefore does not explain this sample's gain. This is intended to
preserve search behavior, and all 24 warmups and 48 timed pairs matched exactly.

**Cached movement properties were effectively neutral.** The candidate used
0.4% less CPU time by equal-case geometric mean (ratio 0.9957), and 0.8% less
summed CPU time (18.079 to 17.932 seconds). Agent-level changes ranged from 2.7%
faster to 0.6% slower; position-level changes ranged from 3.2% faster to 1.3%
slower. All 24 warmups and 48 timed pairs matched exactly, and reported peak RSS
was effectively unchanged. This sample cannot establish a useful speed gain.
The eliminated capability scans are cheap, and the affected lookups may account
for little total work. The added configuration fields and invariants need a
clear maintenance benefit or stronger evidence before inclusion.

**Blocker bitsets merit another attempt, but the integrated test failed.** In
synthetic component tests using actual movement configurations, nonempty blocker
membership was about 13 times cheaper (roughly 14 ns to 1 ns). The integrated
candidate built, and its first two handcrafted warmups matched. NNUE loading
then correctly rejected changed descriptor feature names. Nested two-step
features use Debug formatting: the old empty set prints `{}`, while the new
derived Debug representation prints a struct. That inadvertently changes the
persisted NNUE feature schema. A set-shaped Debug implementation is prepared,
but was not rebuilt or retested before wrapping up. There is **no accepted
whole-search timing result** for bitsets. Do not bypass the model compatibility
check or deploy this prototype.

**Line occupancy masks have component-level potential.** Masked 36-bit line
queries were about 1.5 times faster for dense synthetic lines, 3.3 times for
sparse lines, and 5.6 times for empty lines. This excludes maintaining masks
through moves, undo, promotions, and intermediate captures, and excludes exotic
movement semantics. It is evidence for a future prototype, not an engine
speedup estimate.

Cached movement-property component lookups were about 5–13 times cheaper than
rescanning capabilities, but the end-to-end result is recorded below. Tiny
operation improvements need not dominate search, and caching adds fields and
requires configurations to stay immutable after construction.

## Scope and confidence

- Six preselected positions: opening, tactical at ply 360, history at ply 2000,
  late positions at plies 792 and 842, and endgame at ply 1146.
- Four frozen agents: BASE_C2S2_A40_Lmate, C2K50A1, NNUE512, and NNUE2048.
  Depths 1–3 were selected by the existing baseline benchmark.
- Two shuffled, order-balanced measured pairs per position/agent, after one
  warmup pair. Exact comparison covers scores, full chosen routes, depth, node
  and quiescence-node counts, static evaluations, and ordered root lines.
- Builds and measurements were sequential, pinned to CPU 11 at niceness 19,
  capped to approximately one quarter of one logical CPU. Intentional stopping
  makes wall times inappropriate; process CPU time was used. Frequency and
  cache interference remain possible under the user's concurrent workload.
- Component tests used 303 piece types, 305 canonical configs (including special
  promoted configs), and 228 blocker sets; query frequencies were synthetic.
- No independent confirmation batch, full correctness suites, timed-search
  strength evaluation, or combined-optimization measurement was completed.
  Reserved positions were not used. Treat successful results as a screening
  signal, not production acceptance.

## Artifacts and next action

`results/` contains raw paired observations, summaries, corpus/model identities,
compiler/source/binary identities, actual tested patches, component sources and
raw results, and resource accounting. Game/model payloads and binaries are not
committed. Paths in frozen plans refer to the original local corpus.

The `.patch` files are authoritative for the measured implementations.
`variants.py` also contains later untested compatibility/test changes and an
untested lazy-label prototype. `rules_probe.rs` and
`repetition_micro_unrun.txt` were prepared but not executed; they supply no
experimental evidence. None should be described as verified improvements.

Next priority: independently confirm lazy leaf checks and run its correctness
suite. Then retest blocker-bitset NNUE compatibility before measuring it. Keep
line masks behind that work because their integration is substantially larger.
Do not add the component speedup factors together.
