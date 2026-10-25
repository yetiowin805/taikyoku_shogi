# Efficient strict game-record loading

Baseline source: `c73a7d5ecdd1f179213c1f5664b39a92178ae26b`.
Candidate source: `dc0989125956eb26129b0ff8fcbd9a40ee8a2358`.

## Method and correctness

A temporary optimized helper calls `training::record::load_game_json` on a
preloaded string. Each process warms decoding once, then times 100 loads.
Six baseline/candidate pairs per case alternate execution order: 18 pairs,
3,600 measured decoder calls in total. File I/O and correctness serialization
are outside timing. `results.json` records every raw pair, file and binary hash;
`statistics.json` adds distributions and paired uncertainty estimates.

Builds use rustc 1.92.0 and the optimized release library; the helper uses `-O -C
lto=thin`. Processes are pinned to CPU 2 on 12th Gen Intel(R) Core(TM) i7-1255U.
The host is shared; frequency and unrelated load are not controlled. No compiler
or tests overlap recorded measurements.

The three inputs are a 212 KB/671-move v2 game, a 1.51 MB/4,419-move v2 game,
and a 955 KB legacy export of the latter. Full serialized-record SHA-256
signatures matched on all 12 existing search-speed corpus games and the legacy
export, including setup and agent metadata.

## Parsing time

Times below are medians of six **100-load batch means**, in milliseconds per
load. They are not individual-request latency samples. The paired ratio is the
geometric mean of within-pair candidate/baseline ratios, and the percentage
reduction uses that ratio; a quotient of the separately reported medians can differ.

| Workload | Pairs | Baseline median (ms) | Candidate median (ms) | Time reduction | Paired ratio [95% CI] |
|---|---:|---:|---:|---:|---:|
| v2-small | 6 | 1.256 | 0.260 | 79.4% | 0.206 [0.203, 0.210] |
| v2-long | 6 | 11.515 | 2.159 | 81.1% | 0.189 [0.184, 0.194] |
| legacy-long | 6 | 7.452 | 3.776 | 45.6% | 0.544 [0.513, 0.580] |

The 95% intervals use 10,000 percentile-bootstrap resamples of six paired log
ratios per case. They describe local timing variation, not a guarantee on other
hardware, inputs or future load. No per-request p95/p99 is inferred from batch means.

## Observed variation

These ranges are across batch means. `statistics.json` also records quartiles,
means and sample standard deviations.

| Workload | Baseline observed range (ms) | Candidate observed range (ms) |
|---|---:|---:|
| v2-small | 1.242–1.299 | 0.255–0.266 |
| v2-long | 11.086–13.430 | 2.111–2.424 |
| legacy-long | 6.877–7.984 | 3.552–4.780 |

## Peak memory

Entries are median peak RSS across six complete helper processes, measured with
`/usr/bin/time -f %M`; memory includes the preloaded input and warmup.

| Workload | Baseline peak RSS (MiB) | Candidate peak RSS (MiB) | Difference (MiB) |
|---|---:|---:|---:|
| v2-small | 4.82 | 3.12 | -1.69 |
| v2-long | 16.70 | 5.39 | -11.31 |
| legacy-long | 14.94 | 4.19 | -10.75 |

## Scope and limits

The implementation restores direct typed v2 decoding, shares game detection with
directory featurization, and uses an `IgnoredAny` presence scan for legacy/error
fallbacks. No intermediate JSON tree is needed. Version presence, including null,
continues to require successful supported-version decoding.

Recognized duplicate JSON fields now fail typed decoding rather than being
collapsed to their last value. Valid schema, metadata, feature rows, sampling,
search and dataset publication remain unchanged.

These are parser measurements, not whole-training or search speedups. The
separate F23 feature-counting comparison in PR #189 saves substantially more time
in the Texel extraction stage: the long game's median extraction falls from
526.5 to 64.6 ms. The NNUE tensor-export path is a different workload.
