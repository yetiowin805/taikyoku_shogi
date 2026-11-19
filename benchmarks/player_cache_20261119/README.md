# Player-owned NNUE caches and combined engine speed

Tested November 19, 2026 against main `8595e0528a52dc568231cfd3616703cc012f6838`. The candidate combines a private NNUE cache for each alpha-beta player, exact signed-byte transformer storage for eligible weights, and direct generation of bent captures. It also includes the prior schema-1 teacher and training supervisor improvements. Only the three engine changes participate in playing a move.

**Recommendation. Use the player-owned cache with the other measured engine changes.** The longer production-worker trial played the same 740 moves per build. Total elapsed time across all 12 runs fell from **81.76 to 61.61 seconds** (24.7% less time, **1.33× throughput**). Total CPU fell from 81.65 to 61.50 seconds. Both totals include startup, verification and loading of the models, game play, writing results and process exit. Time recorded inside the game loop fell from **66.79 to 52.40 seconds** in aggregate (21.5% less). This is a fixed-depth result on capped game segments.

| Production worker workload | Stock → candidate elapsed per run, geometric mean | Elapsed-time reduction (95% game-cluster interval) | Elapsed reduction, summed across runs |
|---|---:|---:|---:|
| Move-number cap 12, 138 moves per build | 2.61 → 2.03 s | 22.2% (8.5–29.0%) | 12.3% |
| Move-number cap 64, 740 moves per build | 5.64 → 4.16 s | 26.3% (21.6–29.9%) | **24.7%** |

Every paired self-play segment produced the same full move records, completed depths, evaluations, node counts and recorded outcome after excluding time and provenance. There were six starts and two opposite-order repetitions per arm at each cap. The long-run internal game-loop geometric means were 4.252 → 3.249 s (23.6% less). The short-loop estimate was noisy, with a 9.8% reduction by paired ratio and a 6.9% increase when summed. I would use the longer result to size the benefit on this host. The combined source is commit `918ac3af6f0dbd77e4c6da56da4f117975de2567`.

## What isolation buys

Each alpha-beta player now owns its own cache for as long as that player exists. Search temporarily installs that player's cached feature rows and multi-piece move deltas in the existing thread-local slot, then restores the previous slot on return or unwind. Network identity is still checked. The scope stays on its originating thread. Separate players can take turns on one CPU, and even players using the exact same network object have separate caches. There is no size-based choice between opponents. Both can retain their normal width-dependent working set. The cost is memory for two active players.

The tournament creates one player per side for the game. The player’s search, analysis and progress-reporting methods share this implementation. A persistent analysis player can reuse its own values between positions. An interface that creates a new player per request, including a fresh GUI analysis job, starts that player cold. Cache isolation concerns NNUE data. The two players can still share hardware resources such as the processor's physical caches.

I compared each side's chronological requests alone in a fresh process with exactly the same requests interleaved in one process. The test used two game records, six turns per game, and four pairings. They included identical weights in both seats, distinct W512 networks, W512/W384, and W512/W1536. All **48 turn-by-turn comparisons** matched on the four counters (row hits, row misses, move-delta hits, move-delta misses), full search signature, and completed depth. Across the interleaved searches there were 898,091 row hits, 113,507 row misses, 235,460 delta hits and 3,769 delta misses. A separate Rust regression exercises distinct players holding the same network allocation and a player resuming on another thread. Timings came from a build without counters. [Diagnostic patch](counters.patch) · [Counter observations](isolation.jsonl.gz)

## Search measurements by model pairing

Six archived games supplied six consecutive positions each. Two fresh-process passes reversed executable order. The four executables were stock, an identical stock control, the previous bent-capture + byte + bounded-spare combination, and the new combined engine. All **864 searches** completed at depth two with matching legal full route, score, static evaluation, ordered root lines, node and q-node counts. The primary summaries below first average request CPU within each six-turn segment or its last four turns, then take paired geometric mean ratios across games and passes. The intervals resample entire games.

| Opponents | All six turns, stock → new (ms CPU/search) | CPU reduction across all six | After both players searched, stock → new (ms CPU/search) | Reduction on returning turns (95% interval) |
|---|---:|---:|---:|---:|
| W512-v3 / W384-v4.5 | 83.51 → 71.76 | 14.1% | 77.51 → 62.21 | 19.7% (14.4–24.3%) |
| W512-v3 / W512-v4.5 | 88.61 → 76.87 | 13.3% | 81.28 → 65.02 | 20.0% (14.6–25.8%) |
| W512-v3 / W1536-v3 | 114.02 → 96.08 | 15.7% | 107.73 → 80.17 | 25.6% (18.1–32.4%) |

The old width limit particularly affected W1536. On its own returning turns it used **139.12 → 99.88 ms** per search moving from the bounded design to player-owned storage (28.2% less CPU). W512 in that same pairing used 61.32 → 59.20 ms. Against stock, the new design improved both sides, 27.7% for W1536 and 22.5% for W512. Retained process memory (proportional set size after six turns) was 986 → 539 MiB for stock versus new, or 526 → 539 MiB for bounded versus new. The byte representation accounts for the large stock-relative memory saving. Across the two narrower pairings stock → new used 469 → 264 and 533 → 295 MiB respectively.

For W512/W384, warm returning turns used 3.3% less CPU than the previous bounded combination. For W512/W512 the initial sequence-timed run showed 6.3% more (ratio 1.063, interval 1.020–1.106). I repeated that pairing with fresh processes, arm order reversed at each matched request across passes, and a second copy of the owned executable. The repeat gave **1.0023** for owned/bounded (0.9876–1.0146) and **0.9954** for owned/identical owned (0.9843–1.0051), over 48 returning-turn sets from six games. There is no reproducible narrow-pair advantage over the bounded version here. The clear benefit of private ownership is identical reuse versus standalone analysis plus the wide opponent's improved return cost.

The six-request benchmark server's whole-process CPU ratios versus stock were 0.713, 0.718 and 0.691 across the three pairs. This includes checkpoint loading, rebuilding six recorded positions, legal-route verification, output, and shutdown. Startup is disproportionately large for such a short process. The production tournament-worker figures above are the better estimate for self-play. Whole-fitter and supervisor time have separate denominators. Prior schema-1 teacher measurements found 69.6% less CPU per candidate-ranking request. No teacher or supervisor percentage has been added to the playing figures.

## Workload and limits

The six replay records were selected from locally available files in the November 8 curated VPS archive, ordered by SHA-256 of `cache-owner-v1/` plus filename. They exclude the previous six-game lean confirmation, use distinct matchups, have 450–2,500 recorded moves and no abort reason. Starting fractions were 0.13, 0.28, 0.43, 0.58, 0.73 and 0.88, rounded down to an even record index. Three records begin at the opening and three start from saved positions. Each replay includes all available moves before the chosen segment. The two standalone checks used the first two selected games. Color assignment alternates across replay games.

The production-child experiment uses a separate checked-in set of six legal starting snapshots from three source games. Game history starts at each snapshot. Each of the three width pairs runs with both color assignments across the six snapshots. The child uses the normal tournament entry point, with a fixed depth, a fixed seed, full legal generation, move choice and application, engine verification and result-file publication. The production move-number cap counts two-step moves twice. The short and long runs use the same starts; they test sensitivity to run length. They are not two independent samples of games.

All builds used the repository's portable release profile, Rust 1.92.0, on one shared Linux i7-1255U. Benchmark children ran on a pinned logical CPU. File cache was warm. Parts of the initial short experiments overlapped other builds or validation on the same machine. The timing controls and balanced orders quantify some of that variation. Bootstrap intervals are descriptive, with six replay-game clusters and only three production-game clusters. Fixed-depth results measure throughput for equivalent work. Playing strength and multi-worker throughput require different tests. Actual v6/v7 weight blobs were unavailable.

## Reproduction

Use a full-history checkout of the pinned base for stock and this branch for the candidate. In both trees run `cargo build --locked --release --example nnue_tournament_bench --bin taikyoku_shogi`. The bounded reference is the prior trial's portable `lean` arm at report commit `4fa6fa3` (bent capture, exact byte, bounded spare). Build the diagnostic executable in another candidate checkout by applying `counters.patch` and rebuilding the example. The [input manifest](inputs.json) pins the locally archived games, descriptors and network blobs. The scripts require Python 3.11+, Linux and allowed CPU IDs. Each output path must be fresh.

```sh
python3 replay.py --assets "$ASSETS" --cpu 6 --out replay.jsonl \
  --arm stock="$STOCK" --arm placebo="$STOCK" --arm bounded="$BOUNDED" --arm owned="$OWNED"
python3 replay.py --assets "$ASSETS" --cpu 10 --out isolation.jsonl --isolation --arm owned="$COUNTERS"
python3 interleaved.py --assets "$ASSETS" --cpu 6 --out interleaved.jsonl --owned "$OWNED" --bounded "$BOUNDED"
python3 game_worker.py --repo "$REPO" --assets "$ASSETS" --cpu 4 --limit 12 --out game-worker.jsonl \
  --stock "$STOCK_GAME" --owned "$OWNED_GAME"
python3 game_worker.py --repo "$REPO" --assets "$ASSETS" --cpu 4 --limit 64 --out long-worker.jsonl \
  --stock "$STOCK_GAME" --owned "$OWNED_GAME"
```

To recompute the report from the bundled compressed rows, run `python3 summarize.py` in this directory. [Full estimates including CPU weights and PSS](summary.json) · [Replay rows](replay.jsonl.gz) · [Tournament child, short](game-worker.jsonl.gz) · [Tournament child, long](long-worker.jsonl.gz) · [Narrow-pair repeat](interleaved.jsonl.gz)

## Validation

On the combined source, Rust formatting and strict all-target, all-feature Clippy passed. Cargo reported 622 tests passed and five ignored. All 262 deploy tests and all 79 NNUE Python tests passed, including collection and training process integrations. Project Ruff formatting and lint passed. The diagnostic patch applies to the combined source and was used only for the isolation check.
