# Real-network NNUE follow-up — October 2026

**Latest:** [the comprehensive second round](COMPREHENSIVE.md) covers the
remaining implementation/build ideas, architecture-cost probes, selected
additional changes, and final combined results. The results below describe the
first-round source at `903986e`; they remain as the baseline for that follow-up.

This study checks the five supplied cloud-session patches against current main
`c639ca2`, using trained networks and complete tournament game histories.
The PR retains batched accumulator updates, exact i16 fused rows with an i32
fallback, full-key capture-delta caching, flat reusable caches, attack/leaf-gate
memo tables, vectorizable input quantization, and runtime-selected dense kernels.
It adds an AVX-VNNI kernel for CPUs without AVX-512.

The production cache capacities remain 16,384 piece rows and 4,096 move deltas.
The 8,192/1,024 alternative was measured but not selected: it trades a little
consecutive-search speed for a much lower memory ceiling.

## First-round production results (`903986e`)

All numbers below compare the final large-cache build directly with main
`c639ca2`; below 1 means faster. **On game sequences, trained W2048 v2 took
0.564x time (1.77x faster), W512 v2 took 0.766x (1.31x), W512 v3 took 0.761x
(1.31x), and W1536 v3 took 0.614x (1.63x).** The supplied 0.59x/0.83x synthetic
game-benchmark claim is therefore supported in scale on this host, but not as a
universal ratio: the shuffled corpus produces smaller gains.

| Engine | Shuffled fixed-depth time ratio | Consecutive game-position time ratio |
|---|---:|---:|
| BASE_C2S2_A40_Lmate | 0.961 | 0.935 |
| C2K50A1 | 0.933 | — |
| NNUE512 | 0.911 | 0.766 |
| NNUE2048 | 0.802 | 0.564 |
| NNUE512_v3 | 1.123 (six-position shallow screen) | 0.761 |
| NNUE1536_v3 | 0.826 (six-position shallow screen) | 0.614 |

The newer W512 model is a useful counterexample to a blanket speed claim:
its six-position shallow screen regressed to 1.123x by equal-position geometric
mean, although summed elapsed time improved to 0.830x. In particular, late-royal
went from 1.63 to 3.18 ms and late-endgame from 5.54 to 10.72 ms. This shows a
small-search/setup tradeoff; it does not establish the precise cause. Consecutive
depth-2 searches on that same trained model improved to 0.761x. Retain this caveat
when evaluating very short analysis requests or cold model switches.

The final-source checks contain 96 measured pairs on the original corpus,
144 on v2/handcrafted game sequences, 24 in the v3 shallow screen, and 96 on v3
game sequences, plus one warmup per case. Every signature matched. The complete
release library suite passed **412 tests**, with four previously ignored tests.
No training-history tests failed in this full clone. Evidence is in `results/`;
`production-source-sha256.json` pins the measured source before the report commit.

## Component screens

These ratios compare each change with its immediate stated predecessor. They
must not be added or multiplied into a predicted total; use the direct final
comparison above.

| Experiment | W512 v2 | W2048 v2 | Comparator / corpus |
|---|---:|---:|---|
| Supplied memo + NNUE bundle | 0.953 | 0.819 | main; 12 shuffled positions |
| Added AVX-VNNI | 0.971 | 0.970 | supplied bundle; shuffled |
| Smaller caches | 0.961 | 0.970 | AVX-VNNI bundle; shuffled |
| Smaller caches | 1.012 | 1.039 | same larger-cache build; game sequences |
| Delta cache enabled | 0.973 | 0.973 | otherwise identical small-cache build; shuffled |
| Delta cache enabled | 0.941 | 0.897 | otherwise identical small-cache build; game sequences |

The memo-only screen saved 2.6%/5.7% for the two handcrafted agents and 2.8% for
both NNUE widths. It predates the Whale-origin guard; the final combined build
includes that guard and is measured directly above. The production PR keeps
only the original large-cache layout and dense inference, with AVX-VNNI added.

## Method

- Intel i7-1255U, CPU 2, Rust 1.92.0, native compilation, release profile with
  thin LTO and one codegen unit. AVX2 and AVX-VNNI are available; AVX-512 is not.
- The existing 12-position corpus includes an opening, tactical sweeps,
  independent middlegames, late royal/endgame positions, and a 2,000-ply history.
  Full original game prefixes preserve repetition and progress-draw state.
  Two handcrafted checkpoints and trained v2 widths 512/2048 use the previously
  selected fixed depths (1–3), unchanged between variants.
- The game-sequence corpus replays six adjacent recorded positions from each of
  four games: opening, middlegame, late-independent, and late-endgame. Each engine
  searches all 24 roots at depth 2 in sequence. These are recorded tournament
  moves, not games newly played by the candidate.
- A supplemental screen uses six positions with trained v3 widths 512/1536.
  Some searches are extremely short, so their equal-position averages are noisy.
- Two repetitions with reversed A/B order, after an untimed warmup of each case.
  Searches run sequentially on the same CPU; no builds run during measurements.
  Model loading, hash validation, and game replay are outside timing; search
  setup, accumulator construction, cache lookup, and updates are inside.
- Every pair must have the same complete chosen route, score, main/q nodes,
  depth, static evaluation, and ordered root-line signature. The harness also
  checks move legality and rejects aborted searches. Ratios are geometric means
  of per-case candidate/baseline wall time, with all cases weighted equally.
  Raw summaries also include ratios of summed time and process peak RSS.
  Intermediate summaries and all plans are gzip-compressed to keep the code
  review focused; final production summaries remain readable JSON.

This is a bounded local performance screen, not an Elo measurement, a confidence
interval, or a claim about every tournament host. Fixed-time searches may use the
saved time to search farther and choose differently. The AVX-512 path is compiled
but cannot be exercised on this host. No architecture or network file changes.

## Correctness and memory details

NNUE rows and deltas compare full feature/change-list identities before use.
Rows whose ability sum exceeds i16 use exact direct i32 arithmetic; accumulator
sums remain i32. Tests compare with an independent scalar rebuild, exercise the
fallback, verify reuse from different starting sums, and reject another network's
parked rows. Dense kernels are compared with scalar dot products at extreme
weights and supported widths. The AVX2 and AVX-VNNI paths execute locally.

The imported memo implementation needed three corrections: a distinct empty
attack slot without discarding a hash bit, a safe 31-bit generation rollover,
and movement-origin information for promoted Whales. Silver Rabbit and Reverse
Chariot both promote to Whale but move differently; the existing TT hash omits
their origin. A small incrementally maintained board key supplements only the
new memo keys. TT keys and search behavior are otherwise unchanged. Position
memos retain the normal 64-bit hash-collision caveat of the existing TT.

Parked NNUE caches use a weak network identity, so they do not retain an obsolete
large weight blob per thread. They are discarded when the model changes. The
maximum row-plus-delta payload is 128 MiB at width 2048 and 32 MiB at width 512,
per searching thread (plus keys, undo storage, and small scratch buffers).
Pages are touched lazily. These are capacity ceilings, not observed RSS deltas.

## Speculative ideas

**Sparse first layer: do not enable for these weights.** Across 166,395 evaluated
positions in the instrumented v2/v3 searches, only 0.25–0.55% of individual inputs
were zero, and no four-input block was entirely zero. This is nowhere near the
high sparsity needed to make zero skipping useful. A standalone transposed
four-input AVX-VNNI kernel matched dense integer outputs, but its isolated timing
was mixed: roughly 1.05x for v2 W512, 1.00x for v2 W2048, 0.94x for v3 W512, and
0.86x for v3 W1536. Because no blocks were skipped, any benefit comes from layout
and instruction scheduling. It is not evidence for sparse inference or for a
whole-engine gain; the production kernel stays dense.

**Smaller caches: a real memory/speed tradeoff.** 8,192 rows and 1,024 deltas cut
the W2048 payload ceiling from 128 to 48 MiB. They improved the shuffled screen
by about 3–4%, but were 1.2%/3.9% slower for W512/W2048 game sequences. Keep this
as a deployment experiment if aggregate worker RAM is the limiting resource.

**Capture sweeps: strongly supported.** Even with the smaller delta table,
89–94% of eligible move updates hit. The measured positions averaged 6.4–9.7
piece changes per move, below the supplied synthetic-study estimates but still a
large amount of repeated work. Against an otherwise identical build with delta
caching disabled, it saved about 3% on the shuffled screen and 6%/10% on W512/W2048
game sequences. Both arms of this isolated comparison used the smaller caches.

**Retraining:** pairwise products plus a 16-wide first layer and a PSQT shortcut
remain the most promising candidate from the supplied quality proxy. That proxy
was not re-run here, and its R² is not playing strength. Compare retrained models
in games before changing inference architecture. The single-perspective
combination remains risky; regional designs had poor supplied proxy results.
The measured accumulator values fit i16, but the accepted network format permits
larger values: that observation is not sufficient to narrow production sums.

**Second-round coverage:** PGO, native builds, huge pages, prefetch, persistent
TT and the other implementation ideas are now measured in
[COMPREHENSIVE.md](COMPREHENSIVE.md). Small leaf networks and other retraining
remain deferred. Persistent TT/search changes still need separate strength
evaluation and the repository's history-freeze workflow.

## Reproduce

The local input game prefixes and trained blobs are not duplicated in the PR.
The corpus files pin their identities and paths; rebase paths on another host.
Network loading verifies each blob's content hash. Copy the same harness into
an isolated checkout of `c639ca2`, then build both checkouts identically:

```sh
RUSTFLAGS='-C target-cpu=native' cargo build --locked --release --example nnue_tournament_bench
RUSTFLAGS='-C target-cpu=native' cargo test --locked --release --lib
python3 benchmarks/nnue_followup/run.py \
  --baseline /path/to/main-benchmark --candidate /path/to/candidate-benchmark \
  --corpus benchmarks/nnue_followup/results/corpus.json \
  --cases benchmarks/nnue_followup/results/cases.json \
  --out /tmp/nnue-repeat --repeats 2 --cpu 2
```

For game sequences use `game-corpus.json`, `game-cases.json`, and `--sequential`;
for newer models use `v3-corpus.json` and `v3-cases.json`, or
`game-v3-corpus.json` and `game-v3-cases.json` with `--sequential`. Output directories must
not exist. The runner saves binary identities, CPU/compiler information, order,
all observations, and summaries. `variants.json` explains intermediate builds;
the patches in `experiments/` preserve the ablations without runtime flags in
production. The historical variant named `final` is the hardened **small-cache**
candidate; the selected large-cache source is named `production`.

For a new census, apply `experiments/census.patch` in a disposable checkout,
build the benchmark, and run `census.py BINARY CORPUS CASES OUTPUT.jsonl`.
Instrumentation is excluded from performance measurements. The stored census
used the small-cache candidate. `summarize_census.py OUT JSONL...` accepts gzip
files and extracts sampled quantized inputs for the microbenchmark:

```sh
rustc --edition=2021 -O -C target-cpu=native \
  benchmarks/nnue_followup/sparse_micro.rs -o /tmp/sparse-micro
taskset -c 2 /tmp/sparse-micro WIDTH NETWORK.bin SAMPLED.inputs
```

The microbenchmark requires AVX2 and AVX-VNNI, excludes weight transposition,
uses activations sampled at power-of-two evaluation counts in real searches,
and alternates kernel order across six repetitions. Its results must not be
substituted for end-to-end search measurements.
