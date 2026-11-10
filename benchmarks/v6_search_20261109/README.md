# v6 search transfers, 2026-11-09

The requested experiment is sixteen distinct search packages on each of
`NNUE_W384_v6A`, `NNUE_W384_v6Cm`, and `NNUE_W512_v6Cm`: 48 new entrants.
`admission-config.json` contains every immutable identity and policy/trial pair.
The existing live admission transaction shares the three original NNUE blobs.
No network, material, or other evaluation weights are changed.

## Selection

The frozen live fit had 3791 completed games, checked at
2026-11-10T06:45:21.696602+00:00. Rank each search package by the highest-rated non-v6
representative, including retired agents. Unmodified networks are excluded:
this selects search changes, not network architectures. All selected sources
were active at selection time. `selection.json` records source identities,
ratings, descriptor hashes, input hashes, and the exact normalization rule.

F is mandatory for the new agents. Normalize L to LF and HL to HLF before
removing duplicate configurations. HLF's strongest source was W384-v4.5;
plain HL therefore does not consume a second equivalent slot. Preserve S as a
real package difference. This admits the next distinct package, HLFS_G3PR,
instead of duplicating the HL/HLF behavior.

The selected suffixes, in rank order:

1. `LF` — source `NNUE_W512_v3_L`, fitted rating 1572.43.
2. `HLF_G3CF` — source `NNUE_W512_v3_HLF_G3CF`, fitted rating 1569.57.
3. `HLF_G3ATK` — source `NNUE_W512_v3_HLF_G3ATK`, fitted rating 1567.81.
4. `HLF_G3PV` — source `NNUE_W512_v3_HLF_G3PV`, fitted rating 1553.97.
5. `HLF_G3RFQH` — source `NNUE_W512_v3_HLF_G3RFQH`, fitted rating 1551.39.
6. `HLF` — source `NNUE_W384_v4.5_HLF`, fitted rating 1550.88.
7. `HLF_G3PVSPV` — source `NNUE_W512_v3_HLF_G3PVSPV`, fitted rating 1547.64.
8. `HLF_G3GA` — source `NNUE_W512_v3_HLF_G3GA`, fitted rating 1545.16.
9. `HLF_G3QH` — source `NNUE_W512_v3_HLF_G3QH`, fitted rating 1542.34.
10. `HLF_G3RF` — source `NNUE_W512_v3_HLF_G3RF`, fitted rating 1540.89.
11. `HLFS_G3QH` — source `NNUE_W512_v3_HLFS_G3QH`, fitted rating 1538.55.
12. `HLF_G3SC` — source `NNUE_W512_v3_HLF_G3SC`, fitted rating 1535.12.
13. `HLFN` — source `NNUE_W512_v3_HLFN`, fitted rating 1533.55.
14. `HLFS_G3CF` — source `NNUE_W512_v3_HLFS_G3CF`, fitted rating 1525.56.
15. `HLF_HR` — source `NNUE_W512_v3_HLF_HR`, fitted rating 1521.65.
16. `HLFS_G3PR` — source `NNUE_W512_v3_HLFS_G3PR`, fitted rating 1520.87.

These ratings are selection evidence, not causal estimates of each search
change's benefit. Different training generations confound some comparisons;
L also gains F during transfer. The new tournament games are the strength test.

## Defaults and compatibility

Fresh Rust search defaults and fresh baseline NNUE exports explicitly use
`q_fix`. Missing experiment fields in legacy serialized checkpoints still
resolve to `baseline`. Existing registered descriptors are never rewritten.
The new LF policy adds L to F without adding H. The 48 transfers now pin `generation: v6_search`, separate from both unmodified
`v6` and the older `search` experiments. The original admission receipt records
the initial search-cohort assignment; `generation-map.json` records the later
requested metadata transition. Explicit admission generations take precedence
over the legacy search default, including packages with no third-wave trial.

The live engine uses a pinned historical source. Its compatibility patch lives
at `deploy/compat/v6-qfix-defaults.patch`; current main already applies the q
corrections unconditionally. Deploy the narrow extension rather than replacing
production with all of main. Running games retain their original executable.

## Validation

Repository checks: cargo fmt, clippy with all targets/features and warnings as
errors, cargo test (567 library tests passed, four ignored, plus binary and
integration suites), full tracked Python-tree Ruff format/check, and 228 deploy
unittests. The tiny training/fork/restart integration verifies new exports write
`q_fix`. Regression tests cover new defaults versus legacy checkpoint identity,
LF policy plumbing, corrected-baseline transfer, and trial=None search cohorts.

The pinned production compatibility source also passes all 28 search-experiment
unit tests, including q cache separation, recursive evasion, and legal-move checks.
`admission-receipt.json` records the completed live transaction: 48 entrants
admitted, bringing the roster to 141. Both production helpers validated all 48
new descriptors. Fourteen legacy fixed-depth comparisons (seven existing
packages on two positions, depth 2) matched every emitted result except wall
clock time. LF completed depth-2 smoke searches on all three requested networks.
All 93 existing entrant definitions and model descriptor hashes stayed unchanged;
the coordinator service was not restarted and assignments remain enabled.
The existing dashboard was refreshed without changing its frozen confidence
section. New entrants join subsequent pairings through the normal scheduler.
No strength or speed gain is claimed by these correctness checks.

## Generation transition

Use the shared scheduler upgrader with `--generation-map generation-map.json`.
It validates every ID/label first, lets current games finish, backs up state,
manifest and configuration, and applies only entrant generation metadata while
the coordinator is stopped. Restart uses the same coordinator and rolling game
engine. Assignment pause/resume uses the existing durable command protocol.
Original model descriptors, network files, game records and ratings evidence
are preserved. The original admission receipt remains historical evidence of
the earlier transaction; a separate receipt records the metadata migration.
