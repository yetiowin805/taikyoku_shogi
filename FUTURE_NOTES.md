# Future notes

Ideas parked after a knockout, not the next patch. Search / eval experiments that are ready to measure still live in [`IDEAS_TO_TRY.md`](IDEAS_TO_TRY.md).

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

**Inspect first (11 fits):** `./deploy/run_top11_texel_fits.sh --games-dir data/raw/tourney/top4-mix-swiss-…` inits each chassis and writes `/Pawn` %Δ under `models/top11-texel/compare.md`. Same features; only piece values move. Use that table to decide whether the large-piece moves look reasonable.

**Later transplant (one shared table, not 11):** if the 11 fits agree on the loud pieces, take one of those tables (or a seed-init fit on the same features) and copy **large pieces only** onto each chassis. Current `texel-fit` only trains the **piece-value** vector (`piece_diff`). PST, tropism, and `two_mover_mob_k` stay at `--init`.

**Transplant:** copy each chassis (PST / trop / average / C2 extras) and replace **large pieces only** (Hook, Cap, Tengu, Peacock, GG, other two-movers / capturers). Leave small pieces on the parent so T150 vs H120 twins do not collapse to one mid table.

**C2K50A1 twin extra:** also move `k`. Rows have no mobility feature, so either add a two-mover-mobility column and fit `k` with the pieces, or keep the piece transplant and **line-search `k`** on the same CE. Prefer the line-search unless `k` is going into Texel for good.

Do **not** bake onto `models/ab-seed.json`. Both the control 11 and the twins should run with dest hang-q on (A+B default).
