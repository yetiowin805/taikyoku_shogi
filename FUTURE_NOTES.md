# Future notes

Ideas parked after a knockout, not the next patch. Search / eval experiments that are ready to measure still live in [`IDEAS_TO_TRY.md`](IDEAS_TO_TRY.md).

## Search performance follow-ups (2026-09-12)

The current speed change implements **cached ordering keys** and **skipping
disabled sibling bookkeeping**. Keep the other proposals separate so we can
measure their marginal benefit after these two changes.

The initial experiments used main `1d8830d`, 18 opening/tournament positions,
11 C2/S2/L/A checkpoints, and one pinned i7-1255U laptop CPU. Three shuffled
depth-2 repetitions measured about **10.3%** less elapsed time for cached keys
and **4.2%** for skipping dormant sibling work, relative to an all-flags-off
experimental control. Fixed-depth scores, chosen moves, returned root lines
and node counts matched. Faster execution can still change timed-search
results by completing more work; these figures do not establish playing
strength or VPS throughput.

The earlier five-prototype combination saved 17.8% across equally weighted
positions (13.3% by total time). That result includes three additional changes
and must not be quoted as the measured gain of these two changes alone.

The clean two-change build was also compared directly with `1d8830d`, without
experimental flags: **17.0%** less time at depth 2 across equally weighted
positions (**12.3%** by total time), and **29.6%** on the selected six-position
depth-3 subset. Both used three paired repetitions. All 168 fixed-depth
invocations matched in scores, chosen moves, returned root lines and node
counts, including extra depth-3 checks of sibling modes 1–4 on three positions.
The deeper subset was selected for affordable depth-3 searches, so it is not
an unbiased estimate for all positions.

### Tested without a clear overall benefit

- **Indexed root scores:** 0.0% aggregate depth-2 saving; a few wide roots
  improved by several percent. Revisit only with a stronger root-reordering
  cost measurement. Preserve stable ties and the existing first-match
  from/to/promotion identity, which ignores intermediate routes.
- **Count-only mobility:** 0.3% aggregate saving, below the earlier 1–8%
  hypothesis. The prototype removed Range landing-target vectors, not all
  allocation or repeated evaluation. Incremental mobility is a separate,
  harder experiment: every ray affected by intermediate captures must be
  invalidated.
- **Quiet diagnostics:** -0.8% aggregate saving, within run variation. The
  prototype suppressed eager move labels and progress messages. Lazy
  formatting that retains the messages, and throttled deadline checks,
  remain untested. Any clock experiment must measure timeout overshoot.

### Unmeasured candidates

The ranges below are the original engineering hypotheses for whole-search
elapsed-time reduction, **not measured gains or confidence intervals**. Zero
gain or regression remains possible, and gains overlap. Start with tactical
discovery reuse before changing pruning or search order.

- **Reuse leaf-gate / qsearch tactical discovery (3–12%):** share promotions,
  royal takes and eligible hanging captures for the same position and policy.
  Preserve candidate order and mandatory last-royal evasions.
- **Constant-time board piece-list updates (3–10%):** index squares to slots.
  A naive swap-remove changes traversal order, floating-point accumulation
  and potentially move choice; validate complete make/unmake restoration.
- **Reusable per-ply move/undo buffers (2–8%):** reduce allocation without
  changing candidate order, route ownership or capture capacity.
- **Incremental repetition counts (0–8%, especially long games):** retain the
  entire inherited history and current search/game repetition rules, including
  null-move push/pop. A map may lose to a short vector scan in openings.
- **Borrowed virtual-board attacker iteration (1–6%):** avoid cloning armies
  for simulated capture checks. Validate every removal, promotion and route;
  this overlaps with the ordering-key improvement.
- **Reuse TT storage (0–2% at a 3-second budget):** reuse capacity while keeping
  each search logically empty. Retaining entries across turns is a separate
  behavior-changing experiment.
- **TT-first staged generation (5–20% when effective):** avoid full generation
  before an immediate cutoff. This changes staging/order and interacts with
  LMR, PV-dependent qsearch and route identity; test playing strength too.
- **Interior PVS / aspiration windows (5–25% when effective):** test separately.
  PV-dependent q-depth can change the selective tree; volatile scores and
  repeated re-searches can make either approach slower.
- **Target-directed multi-leg capture generation (3–15% on relevant boards):**
  avoid enumerating routes that miss the target. Require the complete ordered
  candidate sequence to match before treating it as behavior-preserving.
- **Analyzer replay/process batching (0–5% for typical 30-second searches):**
  share immutable game parsing and replay prefixes. This affects catalogue
  throughput, not tournament search; preserve worker isolation, model/cache
  identity, durable iteration results and watchdogs.

For later experiments, reuse varied opening, promotion-heavy, long-history
and last-royal positions with the original checkpoints. Compare fixed-depth
results first, then repeat paired timing at fixed depth and 3/30-second
budgets. Report completed depth and score/move changes alongside node counts.
The local corpus was purposefully varied, not weighted to live-game frequency.

## Top-11 Texel twins (22-agent knockout)

**When:** after looking at hang-q A/B games, and after A+B is default search on `main`.

**Field:** the mix-tournament top 11 (not leftover-only history), each with a Texel twin → 22 agents, play-in bracket.

| # | Chassis |
|---|---|
| 1 | `T150_P120_T12` |
| 2 | `H120_P120_T15` |
| 3 | `AVG_T150_H120` |
| 4 | `C2K50A1` |
| 5 | `BASE_P120H50B75` (`H105_P120_T15`) |
| 6 | `BASE_H120O80` (`H120_OLD_T15`) |
| 7 | `SEED` |
| 8 | `H120_B65_T12` |
| 9 | `AVG_P120_SEED` |
| 10 | `T150_B65_T12` |
| 11 | `C2K100A1D50` |

**Inspect first (11 fits):** `./deploy/run_top11_texel_fits.sh --games-dir data/raw/tourney/top4-mix-swiss-…` (add `--skip-featurize` if `data/derived/top11-texel` already exists). Each chassis `--init`, same features. **Only range two-movers + range capturers move**; Golds / pawns / royals stay on the parent. `/Pawn` %Δ in `models/top11-texel/compare.md`. An unconstrained all-piece fit collapsed Hook/VG to ~0 and exploded the mid table — do not use that.

**Later transplant (one shared table, not 11):** if the 11 fits agree on the loud pieces, take one of those tables (or a seed-init fit on the same features) and copy **large pieces only** onto each chassis. Current `texel-fit` only trains the **piece-value** vector (`piece_diff`). PST, tropism, and `two_mover_mob_k` stay at `--init`.

**Transplant:** copy each chassis (PST / trop / average / C2 extras) and replace **large pieces only** (Hook, Cap, Tengu, Peacock, GG, other two-movers / capturers). Leave small pieces on the parent so T150 vs H120 twins do not collapse to one mid table.

**C2K50A1 twin extra:** also move `k`. Rows have no mobility feature, so either add a two-mover-mobility column and fit `k` with the pieces, or keep the piece transplant and **line-search `k`** on the same CE. Prefer the line-search unless `k` is going into Texel for good.

Do **not** bake onto `models/ab-seed.json`. Both the control 11 and the twins should run with dest hang-q on (A+B default).
