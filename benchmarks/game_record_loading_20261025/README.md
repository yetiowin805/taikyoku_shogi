# Game record loading after strict version validation

Baseline: `c73a7d5ecdd1f179213c1f5664b39a92178ae26b`.
Candidate source: `dc09891` (the results file records the full revision).
Compiler: rustc 1.92.0; optimized release library and optimized helper, one pinned CPU (2).

The temporary helper calls `training::record::load_game_json` on a preloaded string.
Each process warms the decoder once, then measures 100 loads. Six baseline/candidate
pairs per case alternate execution order. File reading is outside timing. Peak RSS
is measured for each complete helper process with `/usr/bin/time -f %M`.
`results.json` records input, helper, and binary hashes plus every paired measurement.

- 212 KB v2 game: median 1.256 to 0.260 ms/load; paired time ratio 0.204.
- 1.51 MB / 4,419-move v2 game: 11.515 to 2.159 ms/load; ratio 0.190.
- Legacy export of the same 4,419-move game: 7.452 to 3.776 ms/load; ratio 0.528.
- Long v2 helper median peak RSS: 16.70 to 5.39 MiB.
- Full serialized record SHA-256 signatures matched on all 12 existing search-speed
  corpus inputs and the legacy export, including setup and agent metadata.

The fix restores the ordinary v2 direct-deserialization path and removes the
featurizer's additional JSON-tree pass. Legacy fallback scans only field presence
with `IgnoredAny`, then decodes the legacy schema without building a generic tree.

These are parser measurements, not end-to-end training or search speedups. The
existing feature extractor took about 563 ms on the long game in a separate initial
screen, so its repeated piece-list copies are a larger preparation opportunity.
Malformed/version-mismatched records remain rejected. Recognized duplicate JSON
fields are now rejected by typed decoding rather than collapsed to their last value.
