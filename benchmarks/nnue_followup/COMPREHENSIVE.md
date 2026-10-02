# Remaining speed ideas: real-network follow-up

This continues PR #137 from `903986e`. No NNUE weights were trained or changed.
Except where a comparison explicitly says otherwise, ratios compare an isolated
experiment with that first PR revision, **not** with pre-PR `main`. Lower wall
time is better. Do not multiply independently measured improvements.

The screen uses the same content-pinned tournament prefixes and trained models
as [the initial study](README.md). It alternates paired searches on CPU 2 after
warmup and requires identical complete move routes, scores, root-line signatures,
main nodes, qnodes, and depth. Two repetitions are a bounded performance screen,
not a confidence interval. Promising changes also get consecutive game prefixes
and newer v3 networks. Loading/replay are outside timing; search setup is inside.
Builds, unit tests, and timed searches share an exclusive CPU-work lock.

The TT research prototypes deliberately change search behavior. Their separate
runner reports disagreements and completed depths rather than asserting parity.
These experiments do not measure playing strength.

## Selected combination vs original main

The selected source is `cf85eb3`: the original PR speedups plus exact feature-row
sharing, root-accumulator reuse and capture-query memoization. Local deployment
builds now target the host CPU; PGO is optional. Neither architecture nor weights
change, and the behavior-changing TT prototypes are excluded.

Both arms below use native builds. The comparator is original main `c639ca2`,
not the first PR revision. These are direct paired measurements.

| Engine | Broad/shallow time ratio | Consecutive-position time ratio |
|---|---:|---:|
| BASE_C2S2_A40_Lmate | 0.943 | 0.996 |
| C2K50A1 | 0.893 | — |
| NNUE512 | 0.882 | 0.725 |
| NNUE2048 | 0.727 | 0.509 |
| NNUE512_v3 | 1.116 | 0.740 |
| NNUE1536_v3 | 0.721 | 0.582 |

W2048 v2 is **1.97x faster** in the consecutive-position benchmark; W512 v2
**1.38x**, W512 v3 **1.35x**, and W1536 v3 **1.72x**. These sequences keep the
same model per thread. Workers alternating between different NNUE models may
lose parked-cache reuse; the shuffled/model-switching screen is a useful
counterpoint. These are replayed searches, not playing-strength matches.

**Remaining short-search regression:** v3 W512 has a 1.116x equal-position mean
in the six-position shallow screen, despite summed time improving to 0.811x.
The two short late positions took 2.16→5.87 ms and 8.31→16.99 ms in this round.
Root reuse did not eliminate that setup/cache tradeoff. Its four longer cases
improved by about 23–25%, and the same model improved to 0.740x on game sequences.
Handcrafted BASE was roughly neutral on game sequences in this final run, while
the broad handcrafted screens improved 6% and 11%. Do not promise universal gains.

On the **selected source**, PGO additionally reduced held-out equal-position
NNUE time by **6.8% at W512 and 5.8% at W2048**; handcrafted means improved
11–13% (the heavier C2K50A1 summed-time gain was 3.3%). This is measured against
selected native, not multiplied into the main-comparison table. The full PGO
workflow built engine, analyzer and benchmark successfully. Expected warnings
for unprofiled CLI/server functions remain in the build log; search is the
profiled workload. It does not prove every application path benefits.

Validation: **414 release library tests passed, four pre-existing ignored**.
All **360 selected-vs-main pairs** plus warmups, and **72 selected-PGO pairs**
plus warmups, matched the complete search signatures. The native helper was
also built with explicitly empty encoded Rust flags and its compiler command
verified to contain `target-cpu=native`. Deployment shell syntax checks passed.
The original user checkout was not changed. No deployment or NNUE training ran.

NNUE row/delta payload capacity remains up to **128 MiB per thread at W2048**,
lazily touched, plus memo tables/scratch/undo storage. The new capture memo is
bounded to 4,096 entries and at most 128 full routes per entry; route buffers
are allocated only as populated. Sparse cached queries may not justify that
overhead on every workload, as the neutral v3 game screen shows.

<!-- MEASURED SCREEN -->
## Isolated implementation screens

Ratios below are paired geometric means. Native-build uses a portable-build
baseline; PGO uses nine held-out prefixes; other rows use twelve prefixes.
The [raw runs](results/comprehensive/runs.json) link binary identities, plans,
full signatures, process peak RSS, and summed-time ratios. Small differences
near 1.00 are not firm wins.

| Change | W512 v2 time | W2048 v2 time | Decision |
|---|---:|---:|---|
| [Host CPU vs portable build](results/comprehensive/native-screen/summary.json.gz) | 0.920 | 0.823 | Use for local deployment |
| [PGO vs native build (held-out)](results/comprehensive/pgo-screen/summary.json.gz) | 0.914 | 0.911 | Opt-in build workflow |
| [Fat LTO](results/comprehensive/fat-screen/summary.json.gz) | 1.001 | 1.006 | Neutral; retain ThinLTO |
| [Fat LTO + abort on panic](results/comprehensive/fat-abort-screen/summary.json.gz) | 0.983 | 0.978 | Mixed; removes recovery |
| [Canonical feature-row cache](results/comprehensive/canonical-screen/summary.json.gz) | 0.993 | 0.899 | Selected |
| [Reuse root accumulator](results/comprehensive/root-reuse-screen/summary.json.gz) | 1.002 | 0.973 | Selected |
| [Capture-query cache](results/comprehensive/capture-screen/summary.json.gz) | 0.955 | 0.983 | Selected |
| [Direct promotion rays](results/comprehensive/promotion-screen/summary.json.gz) | 0.993 | 1.003 | Neutral; experiment only |
| [Evaluation cache](results/comprehensive/eval-cache-screen/summary.json.gz) | 1.014 | 1.032 | Mostly slower |
| [Deferred accumulator updates](results/comprehensive/lazy-screen/summary.json.gz) | 0.999 | 0.985 | Neutral/mixed |
| [i16 cached deltas, initial](results/comprehensive/i16delta-screen/summary.json.gz) | 0.981 | 1.051 | W2048 regression |
| [i16 cached deltas, fused check/write](results/comprehensive/i16delta-fused-screen/summary.json.gz) | 0.976 | 0.978 | Small gain; memory tradeoff |
| [Prefetch capture rows](results/comprehensive/prefetch-screen/summary.json.gz) | 0.999 | 0.978 | Marginal; experiment only |
| [Huge pages for weights](results/comprehensive/hugepages-weights-screen/summary.json.gz) | 0.981 | 0.976 | Mixed across workloads |
| [Huge pages for weights + caches](results/comprehensive/hugepages-screen/summary.json.gz) | 0.960 | 1.102 | W2048 regression + extra RSS |
| [Transposed dense head](results/comprehensive/transposed-screen/summary.json.gz) | 1.000 | 1.002 | No engine gain |

Consecutive game prefixes temper several screen results. Canonical rows save
about 1–4% across the four v2/v3 models once warm, versus 10% in the broad W2048
screen. Capture memoization saves 2–3% in the v2 game sequences and is roughly
neutral in v3 (0.2–1.3% slower equal-position means, about 1% faster summed time).
Its handcrafted broad-screen gain is 4–5%. Root reuse saves 6% in the v3 W1536
shallow screen and about 2% at v3 W512; the W512 v2 screen is neutral.

PGO saves 8.5% in both v3 game sequences. On the held-out handcrafted prefixes,
equal-position means improve 12–14%; the C2K50A1 summed-time gain is only 1%,
so that average should not be described as a universal 12% game-time saving.

The refined delta implementation removes an avoidable scalar fit scan. It
saves 2.2–2.4% in the v2 screen, but only 0.5–1.4% in v3 game sequences. Narrow
row-plus-delta capacity falls from 128 to 96 MiB at W2048; if every delta slot
needs its exact i32 fallback, the ceiling instead becomes 160 MiB. These are
payload capacities, not observed RSS savings. This remains a deployment tradeoff.

Weights-only huge-page advice really allocated 928 MiB of huge pages with
essentially unchanged RSS. Its 2% v2 gain did not repeat in v3 game sequences
(W512 1.018x, W1536 0.999x). Advising caches too added 89.5 MiB RSS after just
three opening searches and regressed W2048 by 10%. This is why huge-page advice
is not enabled by default. [Actual page coverage](results/comprehensive/memory-hugepages-coverage.json).

The transposed four-input head was neutral for v2 and 3% slower for v3 W1536
in search despite its earlier isolated-kernel promise. Real activations have
only 0.25–0.55% zeros and no all-zero four-input blocks: sparse inference still
has no useful sparsity to exploit. Prefetch is a marginal W2048-only screen win
and has not been selected or combined with canonical row identities.

<!-- /MEASURED SCREEN -->

## Architecture costs without retraining

`architecture_micro.rs` uses actual recorded W2048 activations and deterministic
synthetic dense-head weights. It compares instruction/matrix costs, not random
network search trees or quality. Dense dot products do not depend on weight
quality; random-network search trees would make engine speed comparisons
misleading. The input width variants slice the recorded activations; they are
not smaller trained networks.

Pairwise inputs use vectorizable fixed-point products `(a*b+64)>>7`, with a
reused output buffer. The PSQT experiment measures four already-accumulated
linear lanes per perspective at output, and their extra cached-delta additions.
It excludes the cost of loading/fusing those lanes on uncached feature updates.
Neither this probe nor the supplied handcrafted-evaluation proxy establishes
playing strength. PSQT, pairwise products, narrower heads/accumulators, single
perspective, and a separate small leaf network still require training and games.
Regional accumulators remain low priority because of the supplied proxy loss;
that quality experiment was not repeated.

The guarded-i16 update probe compares one hot cached-delta operation against
i32, checks an overflowing case, and retains an overflow escape. It excludes
narrow/wide undo-stack conversions, uncached feature updates, and cold cache
traffic. Existing model files allow values outside i16: unconditional narrowing
is incorrect even if all sampled production sums fit. Conservative min/max
bounds carried with each accumulator/delta are another possible guard, with
false fallbacks as the bounds widen. A full engine implementation remains an
experiment rather than a claim that retraining is mandatory for exact i16.

<!-- ARCH COST -->
| Synthetic shape | Head time ratio | Hot cached-update ratio |
|---|---:|---:|
| baseline | 1.000 | 1.000 |
| h16 | 0.501 | 1.008 |
| pairwise | 0.561 | 0.991 |
| pairwise-h16 | 0.335 | 1.019 |
| psqt | 1.020 | 1.035 |
| pairwise-h16-psqt | 0.329 | 1.016 |
| single | 0.528 | 0.426 |
| single-pairwise-h16-psqt | 0.191 | 0.458 |
| width1024 | 0.541 | 0.436 |
| width512 | 0.289 | 0.211 |

Baseline isolated head: 1.912 µs; cached update: 0.364 µs.
The guarded-i16 hot update is 0.548x i32 in this probe; it is not a whole-engine result.
Six rotating/reversed repetitions are retained in [raw micro timings](results/comprehensive/architecture-micro.jsonl).

<!-- /ARCH COST -->

## Build experiments

`deploy/build_native.sh` is now used by the local pull/build tournament wrappers,
grid preparation, and engine-update build. It targets the machine doing the
build; use ordinary Cargo for a portable binary intended for another CPU. It
preserves caller flags, including an explicitly empty encoded-flags variable,
and build errors remain fatal. No tournament was started, stopped, or updated.

`deploy/build_pgo.sh CORPUS NEW_OUTPUT_DIR LLVM_PROFDATA --positions ... --models ...`
provides the optional two-stage compiler-profile build. It requires matching
LLVM versions, pins profile inputs by hash, preserves panic recovery, and writes
standalone engine/analyzer/benchmark binaries and provenance under the output
directory. It never installs them. `--cpu 2` reproduces this host's affinity;
otherwise the workload selects the first allowed CPU.

The baseline already uses ThinLTO, one codegen unit, and `target-cpu=native`.
Fat LTO alone was neutral. Fat LTO plus `panic=abort` was mixed, and abort would
remove the recovery used by `catch_unwind` in server and tournament workers.
Neither is selected from these results.

PGO here means **compiler profile-guided optimization**, not NNUE training.
Its profile uses 360 depth-2 searches: six existing models, three tournament
prefixes (indices 0, 1, 6), twenty passes. The main comparison excludes those
three positions. Supplemental game sequences may overlap the profile games;
they are not an independent held-out corpus. The matching LLVM 21.1.3
`llvm-profdata` came from the installed Rust 1.92.0 toolchain's official component
manifest; the download identity is recorded with the results. Profiles are
specific to source/toolchain/workload and are not committed as production data.

To reproduce, build the same source with the same target flags in two stages:

```sh
RUSTFLAGS='-C target-cpu=native -C profile-generate=/tmp/nnue-profile' \
  cargo build --locked --release --example nnue_tournament_bench
cp target/release/examples/nnue_tournament_bench /tmp/nnue-instrumented
python3 benchmarks/nnue_followup/profile_workload.py \
  --binary /tmp/nnue-instrumented --corpus /path/to/v3-corpus.json \
  --positions 0 1 6 --models 0 1 2 3 4 5 --repeats 20 \
  --out /tmp/nnue-profile-run
llvm-profdata merge -o /tmp/nnue-merged.profdata \
  /tmp/nnue-profile-run/profiles/*.profraw
RUSTFLAGS='-C target-cpu=native -C profile-use=/tmp/nnue-merged.profdata -C llvm-args=-pgo-warn-missing-function' \
  cargo build --locked --release --example nnue_tournament_bench
```

Use a matching `llvm-profdata`, run representative workloads, and keep held-out
positions for the performance comparison. Network/game hashes and raw compiler
profile searches are recorded. Independent exported source directories must use
separate Cargo targets or `cargo clean --release -p taikyoku_shogi` before each
build; otherwise copied timestamps can reuse a stale normal-library artifact.
Save immutable executables and verify their hashes before timing.

## Search behavior research

Persistent main TT retains draw progress in its key. The next played position
can already match a node from the previous search with the same counter, whereas
removing the counter can reuse bounds across different progress-draw outcomes.
The prototype salts movement variants and resets on model/game/nonconsecutive
position changes. It does not persist qsearch bounds, which depend on q context.
It still needs an owned game/evaluator/config session, repetition-history policy,
and strength validation before production.

The staged TT picker generates the hash mover first and can avoid generating
the whole army on a cutoff. Searching that move first changes the history seen
by later ordering and selective search. Faster or smaller trees alone cannot
establish that it is better. These behavior-changing ideas need game matches
and the repository's logic-history snapshot when eventually merged.

Material-only qsearch cutoffs also need care: a proven lower bound can establish
a fail-high without computing the residual, but returning that bound changes
fail-soft scores and TT information. It is not automatically an exact-speedup
replacement. The diagnostic census distinguishes guaranteed bounds from exact
output-clamp shortcuts and tracks lazy updates needed by descendants.

The staged-picker comparison completed seven of nine requested depth-3
handcrafted cases in both arms. Among completed pairs it was 1.077x handcrafted time and
0.974x W2048 time. At a one-second depth-4 cap it gained no completed-depth
cases. All same-depth choices and scores agreed in this small experiment;
that does not make the algorithm generally behavior preserving.

Persistent main TT was more interesting: 0.728x handcrafted time and 0.982x
W2048 time among completed depth-3 pairs. Some moves/scores changed. At the
one-second depth-4 cap, handcrafted gained one deeper case and NNUE lost one:
a late NNUE endgame went from depth 4 in 300 ms to only depth 3 after 1,000 ms.
There were 72 legal research pairs across both prototypes and both budgets.
These results justify further research, not a production TT-policy change.

## Census and remaining work

The deeper census covered 166,395 residual evaluations over 36 real-network
cases, all matching the recorded production search signatures. Only 3.9–10.9%
of accumulator moves were never consumed by either that node or a descendant;
they contained just 3.0–5.9% of changed pieces. Position repeats were 1.8–5.0%.
The implemented residual cache and deferred accumulator variants did not earn
their overhead. Rigorous material bounds permitted only 17 q shortcuts, and
there were zero exact output-clamp shortcuts.

All examined sums fit i16: the full observed range was -3606 to 7023 across
302 million evaluated channels and 333 million restored-move channels. This
also bounds every observed move delta within i16, but is not a model-format
guarantee. Diagnostic scans perturb caches; sampled routine timings are not
used as end-to-end speed evidence. See the [census summary](results/comprehensive/census-summary.json)
and [definitions](experiments/comprehensive/EVAL_NOTES.md).

78–90% of residual evaluations occurred in frames that made no child. This is
an after-the-fact count: many cutoff leaves cannot be recognized until after
evaluating them. A small network for known q leaves or lopsided positions still
needs training, a switching rule, and games. The count alone does not establish
how many calls can safely use it.

Coverage of the original ideas:

- **Measured with real nets:** imported memo/NNUE changes in the initial study;
  full-move delta caching and cache capacities; native builds, PGO, LTO/abort;
  row sharing/prefetch/huge pages; exact narrow delta payloads; residual caching;
  deferred updates; capture-query caching and direct promotion rays; staged and
  persistent TT research. Root-accumulator reuse was discovered during this work.
- **Measured only as kernel costs:** pairwise products, 16-wide heads, their
  combination, PSQT output lanes, single perspective, narrower widths, and a
  guarded i16 accumulator update. None is an end-to-end architecture/strength
  result. A transposed dense head was additionally tested in the real engine
  and rejected; a full sparse path has no measured sparsity to exploit.
- **Bounded and dropped:** material-only skipping, based on the tiny rigorous
  cutoff opportunity; regional/hybrid designs remain dropped based on the
  supplied quality-proxy loss, not a newly repeated quality study.
- **Deferred explicitly:** retraining and playing-strength tests for alternative
  heads, PSQT/pure-linear models, smaller leaf networks and model switching;
  full guarded-i16 accumulator/undo storage; session/history-safe persistent TT;
  AVX-512 execution on a supporting CPU. The supplied R² quality studies were
  not repeated. This host can exercise AVX2/AVX-VNNI, not AVX-512.

The independent [experiment patches](experiments/comprehensive/) apply to
`903986e`. They preserve rejected/provisional implementations without adding
runtime toggles to production. Patches are evidence and starting points, not a
recommendation to enable all of them together.
