# Independent memory experiments on PR #137 (`903986e`)

Each patch applies separately to the unchanged PR source. None needs retraining,
and none narrows accumulator sums (which remain i32). `canonical` and `i16delta`
include independent scalar-rebuild tests; all variants require paired tournament
signature verification before performance acceptance.

Build configuration: Rust release profile, `RUSTFLAGS='-C target-cpu=native'`,
`CARGO_TARGET_DIR=/tmp/nnue-speed-target`, `cargo --locked --offline`.
All builds and tests use `/tmp/nnue-comprehensive/cpu.lock`. Each reported
benchmark binary is built after `cargo clean --offline --release -p
taikyoku_shogi --target-dir /tmp/nnue-speed-target`: shared target reuse across
source exports previously produced stale normal-library artifacts. Earlier
pre-clean binaries are discarded. `READY.json` lists only verified clean builds. Finished benchmark
binaries are copied to `/tmp/nnue-comprehensive/bin/memory-VARIANT` before unlocking.

## Canonical fused-row identities

Cache key becomes `(ability-list equivalence class, relative color,
perspective-adjusted square)`. Different piece identities and opposite
perspectives reuse one row only when their complete selected feature lists are
identical. This preserves White-jump quirks and promoted Reverse-Chariot origin.
The original schema hash/model format is unchanged; the equivalence classes are
inference metadata derived after that hash is computed.

The added test checks every piece/promotion/origin combination at mirrored
squares against the actual feature-list identities. Existing scalar-rebuild,
collision, wide-row, cache-lifetime, and move-delta tests also pass. Timing could
still regress because cache-slot distribution changes and lookup gains may be
small after move-delta caching.

## Exact i16 move deltas with i32 fallback

A cached move's combined delta uses i16 when **every lane** fits. An overflowing
slot stores a separate exact i32 payload; hits dispatch on whether that payload
exists. Replacing a wide slot with a narrow one frees its wide allocation.
Accumulator sums, snapshots, and direct fallback arithmetic remain i32.

At W2048, narrow row+delta payload ceilings are 64 + 32 = 96 MiB versus 128 MiB
before. If every delta slot is wide, the worst-case payload rises to 160 MiB
because the narrow backing remains allocated. This is a real caveat: observed
RSS and real-model fallback occurrence matter more than the best-case ceiling.
The separate every-move census found per-model accumulator ranges
W512 -2251..4549, W2048 -2137..6666, v3-512 -3087..5776, and
v3-1536 -3606..7023 across 36 cases. The widest range spans only 10,629, so
every observed new-old delta in that census fits i16. This is empirical evidence
for those cases, not a bound on every loadable network or possible position.
See `/tmp/nnue-comprehensive/eval-artifacts/census-summary.json`.
The added test forces a five-piece delta beyond i16 even though its individual
fused rows fit; miss, repeat hit, undo, and parked-cache reuse must match an
independent scalar rebuild.

## Transparent huge pages

Linux best-effort `madvise(MADV_HUGEPAGE)` is called on full pages wholly contained
inside buffers of at least 4 MiB, before their first writes. There is no global
system setting change, `mlock`, explicit hugetlb allocation, privileged call, or
unsafe read/write. Non-Linux targets do nothing; failed advice is harmless.

`hugepages-weights` advises only the large feature matrix.
`hugepages` additionally advises fused-row and move-delta backing buffers.
THP settings on this host were `[madvise]`, defrag `[madvise]`, with 2 MiB pages.
Actual coverage must be checked through `/proc/PID/smaps_rollup` while the server
is alive. Cache advice can increase RSS by faulting 2 MiB around sparse slot
accesses; this warrants measuring both variants instead of bundling them.
The search harness excludes network loading, so any change in initial weight
allocation/compaction latency needs separate interpretation. Cache faults occur
inside the measured search and are included.

## Software prefetch

The x86_64-only prototype prefetches the first 128 bytes of every changed piece's
cached fused row, or each source feature row on a cache miss, before a batched
move update. It executes after a combined move-delta miss, so repeated sweeps
already serviced by the delta cache bypass this extra work. Hardware prefetch
handles contiguous row tails. Unsupported architectures use a no-op.

This may lose: additional key hashing, cache probes, and feature enumeration can
cost more than the latency hidden. Retain only if paired real searches support
it; a microbenchmark alone is insufficient.

## Four-input transposed dense head

`transposed` stores an additional input-major h1 matrix at model load (128 KiB
at W2048). It uses four-input broadcasts into four independent AVX-VNNI vectors,
so the 32 output sums need no horizontal reductions. There is no zero-skipping
branch; previous activation samples were dense. The existing AVX-512 preference
and portable/non-VNNI kernels remain unchanged. File layout, quantization,
activations, and arithmetic remain unchanged. Scalar parity is checked at
supported widths 32–4096, all-zero/127 inputs, signed weight extremes, and mixed
inputs and weights. This is a whole-engine follow-up to sparse_micro.rs's earlier
isolated layout result, not a claim that these networks are sparse.

## Refined i16 delta storage

`i16delta-fused` replaces the initial prototype's potentially scalar early-exit
fit scan with a single narrowing-write plus OR-reduction pass. The reduction
ORs `(delta.wrapping_add(32768) as u32) >> 16`; this is zero exactly when every
original delta fits i16. If any lane overflows, the full i32 payload is filled
and used on hits; narrowed overflow bytes are never consumed. This avoids an
extra full scan on the common narrow insertion path and permits vectorization.
All initial prototype semantics, fallback tests, and memory caveats still apply.

## Verified THP coverage and memory cost

A bounded live-process check used the baseline and both THP builds on identical
W2048 opening roots 0, 1, and 2, depth 2, then read `/proc/PID/smaps_rollup`.
Every search signature matched. Baseline: 0 KiB anonymous huge pages, RSS
1,027,824 KiB. Weights-only: 950,272 KiB anonymous huge pages (928 MiB), RSS
1,027,856 KiB. Weights plus caches: 1,077,248 KiB anonymous huge pages (1,052 MiB),
RSS 1,119,492 KiB. All had zero swap. Thus weight advice actually worked without
material RSS growth; adding cache advice cost about 89.5 MiB already after three
opening searches. This check establishes allocation behavior, not a timing
conclusion. Raw source, requests, signatures and memory fields are retained in
`hugepages-corpus.json`, `check-pages.py`, and `hugepages-coverage.json`.

All seven final binaries are listed in `READY.json` with verified SHA256 values,
independent source patches and manifests, and clean build logs. No production
source in the root checkout was modified by this agent.
