# Ideas to try (search, quiescence, eval)

Notes from knockout games. **Nothing here is implemented yet.** Pick one experiment, measure it on the check positions plus q-node counts, then bake. Do not stack A+B+C in one patch.

A **major eval or search-behavior change** (new term, new hang-q rule that changes move choice under the same time/depth) needs a `kind: logic` history freeze on the parent of the merge — see `AGENTS.md`. Tests, scripts, and this file do not.

## Games

All under `data/raw/games/interesting-knockouts/` (gitignored data; catalog in `INDEX.md`).

| Short | File | Event |
|---|---|---|
| **slot0260** | `top4-mix/two-mob/slot0260-C2K100A1D50-vs-BASE_P120H75B60-a-black.json` | Black C2K100A1D50, White BASE_P120H75B60. BlackWins, 1235 plies. |
| **slot0202** | `top4-mix/two-mob/slot0202-C2K50A1D25-vs-H105_P120_T12-a-black.json` | Black C2K50A1D25, White H105_P120_T12. WhiteWins, 1594 plies. |
| **slot0151** | `top4-mix/late/slot0151-H105_OLD_T12-vs-AVG_P120_SEED-a-white.json` | Black AVG_P120_SEED, White H105_OLD_T12. WhiteWins by mate, 844 plies. |

Tournament games: 1s, depth 8, PathAware q. Evals in the JSON are **black-absolute**. Each side annotates its own moves with its own weights. Seed numbers below are `EvalWeights::seed()` with noise off.

**GUI ply** = 1-based index into `moves[]`. `replay_to_ply(n)` applies the first *n* records, i.e. the position **after** ply *n*. To search the move that *is* ply *n*, replay `n-1`.

Approximate seed material (for swing sizes): Hook ~6237, Capricorn ~2970, Peacock ~1663, VG ~1382, GG ~2160, Flying General ~1080, Crown Prince **8**, Treacherous Fox ~60, Mountain Crane ~80. Loud-capture floor ~648. `HANG_NET_FRAC` 0.8, `HIGH_VALUE_HANGER` 400.

## Suggested order

1. **A** — dest-hangs of high-value two-movers open q. Several confirmed misses; smallest patch.
2. **B** — same, but the taker is a capturing-range piece landing on the two-mover (PathClear dest).
3. **M** — last-royal captures are mate/loud (slot0151 mate-in-1 ordering). Tiny search change, separate from eval.
4. **L** — last-royal flight penalty (slot0151 mate while ahead on material). Eval; freeze.
5. **C / F**, then **E**, then **J** — corridor lasers (slot0260 ply 70, slot0202 ply 65–67 / 72 overshoot). Measure q-nodes before letting dest-empty PathClear back into q.
6. **D** only if C is too narrow.
7. **G, H, K, tropism retune** — later / speculative.

---

## Current q contract

After a **quiet** AB parent, `leaf_or_quiesce` stand-pats unless:

1. the parent was a loud capture, or
2. there is a loud promotion (`promotes_into_big_piece`: two-mover or capturing-range, e.g. FreeKing→GG — **not** TreacherousFox→MountainCrane), or
3. `stm_has_large_hang_simple_take` — STM can **SimpleTake** a large enemy with a cheaper mover, `mv.to == victim`, and **not** PathClear/MultiLeg (`quiesce_move_looks_path_or_multileg`).

PathAware q then expands PathClear/MultiLeg only as a **destination recapture** (`mv.to == prev_to`). Corridor wipes (victim on the path, dest elsewhere) are left to main-search depth.

Code: `leaf_or_quiesce`, `stm_has_large_hang_simple_take`, `is_large_hang_simple_take`, `generate_captures_hitting_square`, `pathclear_allowed_in_pathaware_q` in `src/search.rs`. Victim test: `is_large_hang_victim` (`is_big_piece` or ≥ loud floor). Range two-movers: `is_range_two_mover` (Tengu, Capricorn, Hook Mover, Peacock, …).

**The repeating miss:** a quiet parent, then a two-step or capturing-range piece **lands on** a Hook. Hang-q throws that take away. Depth 1 stand-pats (sometimes even *prefers* the hang because PST/mobility likes the new square). Depth 2 sees it. 1s with ~200–1500 root moves often never finishes depth 2, so the depth-1 PV stands.

**Do not conflate with AB hang-skip.** `capture_hangs_high_value_piece` skips *our* hanging **movers** (net < 0.8×mover and landing attacked). Hang-q looks for hanging **victims**. PathClear hang-skip already sums path loot and post-fire confirms (ply-71 false-positive fix: GG through a defending Hook).

---

# Search / quiescence

## A — Always test dest-hangs of high-value pieces

**Do this first.**

The quiet-leaf scan already walks enemy large pieces and asks “can STM land on this square?” It then **drops** every two-step / Free Eagle / PathClear, even when `to` is the victim. That is why a hanging Hook is invisible at depth 1.

Keep the victim loop. For those pieces, **any dest-capture counts**, including MultiLeg. Still ignore path-only wipes (`to` elsewhere). Keep `mover < victim`.

If a full `is_large_hang_victim` pass pulls in too much MultiLeg, gate the extra dest-takes on `is_range_two_mover` (Hook, +Capricorn, Tengu, Peacock).

**Try 1:** only **open** q (`hang_caps`). PathAware inside q can still keep MultiLeg as dest recapture; once q is open and `prev_to` is the Hook, that recapture *is* the landing take.

**Try 2:** also search those landing MultiLegs as q candidates when hang-caps opened.

Not an eval hang-penalty. That would run on every node for every Hook and needs SEE-ish “is it really hanging.” Opening q is the cheap “always check if the Hook is hanging.”

**Risk:** extra MultiLeg in q. Existing PathAware dest-recapture, TopN, hang-net should contain it. Measure q-nodes on a quiet opening **and** on a GG PathClear leaf (must not regress to pre-PathAware blowup).

### Checks

**slot0260 ply 175.** After ply 174: Black Hook on `23,25`, White to answer a quiet two-step `23,25-23,21-22,21`. White Hook `5,10-5,21-22,21` lands on it. Recorded search stayed ~−5900; after the take ~−11700. Q never opened.

**slot0151 ply 153–155.** After ply 152, Black Great Stag `14,3-12,5` (quiet, empty) uncovers Peacock `13,2`. White’s Hook (`+Capricorn`) is on `12,9`. Before the uncover the Peacock has **no** dest-hit on `12,9`; after, the only dest-hit is `13,2-16,5-12,9`. White plays Treacherous Fox `17,27-17,6+` (empty file; TF→MC is **not** a loud promo, 60→80). Search **−7353** vs static **−7311**. Black takes the Hook, eval **+1635**.

Seed: depth 1 *picks* the Fox (score = stand-pat after the slide). Depth 2 runs the Hook (`12,9-12,12`). This position has ~1473 root moves; depth 2 took ~192k nodes. Tournament 1s was ~26k (seed 1s ~57k, abort) and kept the depth-1 Fox.

Expect after A: depth 1 must stop picking `17,27-17,6+` once q sees `13,2-16,5-12,9`; same for the slot0260 Hook-takes-Hook.

---

## B — Range dest-captures of high-value two-movers

A covers two-movers taking by landing. **B** covers BG/GG/VG/Fierce Dragon/etc. **sitting on** the two-mover, including PathClear that wipes the ray and lands on the victim.

Still **not** a PathClear that only wipes the two-mover mid-ray and lands beyond (corridor). That is C–F.

Same contested-square rule as PathAware (`mv.to == prev_to`), but the parent may have been a quiet step onto the square, so `prev_to` q never runs today.

**Risk:** GG/BG dest PathClears were the original q blowup. Gate hard: victim is a high-value two-mover; dest == victim; first try **open q only**.

### Checks

**slot0202 ply 72.** White Hook quiet two-step `16,30-17,30-17,27` onto file 17. Black GG `17,3` already looks through Iron `17,25` / Dark Spirit `17,31` / Phoenix Master `17,33`. The Hook was safe on file 16.

GG **can land** on the Hook (`17,3-17,27`, PathClear through own Daiba + Iron). Tournament Black overshot to `17,3-17,33` (Hook + Dark Spirit + Phoenix Master). Eval **3479 → 7131**. Seed depth 1 from Black prefers `17,3-17,34` at +7162.

Only dest-hit on the Hook is that GG PathClear — no cheap SimpleTake — so hang-q does not open. Seed from White:

- Depth 1 *picks* the hang (`16,30-17,30-17,27`, −3442 vs static −4117). The new square looks ~675 better (Hook PST/mobility).
- Depth 2 scores that line −7162 and plays something else.
- Tournament ~23k nodes, score **3479** ≈ depth-1 stand-pat. Seed 1s (~39k, abort) already had the hook line at −7162 and refused it.

B dest==Hook is enough for White’s depth-1/q to see the piece is lost. D covers the overshoot that was actually played. Hang-skip is not why: Hook loot ≫ 0.8×GG.

**slot0260 ply 70–73** is a corridor (Hook on the path, dest `24,0`). B as stated does **not** pull that line into q.

---

## C–J — Corridor lasers (speculative; after A/B)

Motivating line: **slot0260 ply 70–73**.

- Ply 70: Black Dragon King `23,18-23,25+` (pawn, promo Flying Eagle).
- White BG `31,33-11,13` PathClear (mostly own junk + the new FE). Both sides ~even (**−44 / −95**).
- Black ignores.
- `11,13-24,0+` PathClears Hook + Peacock **on the path** (dest Wooden Dove / promo Rain Demon). Eval **−4395**.

A/B do not help: the Hook is not the landing square.

Likely mechanism: at ID depth 1–2 the follow-up exists only in **q**. After staging, `prev_to` is `11,13`; the wipe lands on `24,0`, so PathAware drops it. AB would see `staging → reply → strike` from depth 3+, but 1s / ~20k nodes may never finish that on a late-ordered BG shuffle. White *played* the staging at root, so hang-skip of the staging move is not the story. Q sort is **landing victim first**, then path-sum — even if the corridor wipe were allowed, TopN=2 could bury `24,0` (Dove ~50).

Prefer tiny gates. Measure q-nodes on a GG PathClear leaf **before** anything that lets dest-empty PathClear back into q.

### C — Fire-from-landing (narrowest q gate)

Allow PathClear/MultiLeg in PathAware q when **`mv.from == prev_to`**: the piece that just landed may **shoot**, even if dest ≠ prev_to.

Gate further: path or dest includes a high-value two-mover / loot ≥ loud floor. One from-square, not every BG on the board.

Ply 73 is exactly “BG that just moved to `11,13` fires down a new diagonal.”

**Risk:** that piece may have many dest-empty PathClears. Budget with `QUIESCE_PATHCLEAR_DEEP_BUDGET` / TopN; maybe only the first q ply.

### D — Path-through-two-mover (wider)

Any PathClear whose **path** contains a Hook/Peacock/Tengu/… (not only the last mover). Still dest-empty / corridor.

Also covers slot0202 ply 72 overshoot `17,3-17,33` (Hook mid-path). **Risk:** classic mop blowup. Try only after C fails, and only to open q or only at q ply 0.

### E — If C/D allow corridor wipes, sort them by path loot

Q sorts landing victim first. Hook-on-path / Dove-on-dest looks like a 50-point take and loses TopN. Companion to C/D, not useful alone: for `CaptureKind::PathClear`, order by `max(landing, max_path)` or `enemy − own` (`tactical_gain`). AB `mvv_lva_score` already uses full path exchange — this is a **q-list** issue.

### F — Don’t stand-pat after a capturing-range PathClear

If `last_ab_wipe` and the mover was a range capturer, skip quiet stand-pat and enter q with C’s from-square rule (or a 1-ply capture gen **only from `last_ab_to`**). Same idea as C, trigger is “parent was a wipe.” Might be easier to A/B test.

### G — Extra ID depth on fat-path PathClear

`move_loudness` already sums path enemy. The miss is q **dropping** the move, not LMR (captures aren’t reduced). Crude: require remaining depth ≥ 2 after a capturing-range PathClear before trusting stand-pat / d=1 Exact. Last among search ideas.

### H — Eval ray-threat through two-movers

Static: capturing-range piece vs enemy Hook/Peacock on a clear diagonal. Staging `11,13` would already look huge **before** the take. Noisy; encourages hanging the BG to “aim”; weights territory. Very tentative.

### I — Skip this miss

Accept 1s horizon on dest-empty PathClear; A/B are the Hook-on-square case. Revisit C–E only if lasers like ply 70 show up often.

### J — Block the laser (even if C–F see the take)

Seeing `11,13-24,0+` is not enough if every root try is an unrelated promo and the **save is a quiet**. After staging, Black had at least:

- own BG `4,2-13,11` (empty square on the threatened diagonal)
- GG `17,3-17,7` (same ray)

Capturing-range pieces are blocked by a small set (BG: set 3 — King, Crown Prince, GG, Flying General, Flying Crocodile, BG, Vice General, Fierce Dragon). A Gold on the ray does not stop the wipe.

Q is capture-only, so it will never play the block. Quiets are LMR’d after the first few. 1s can learn “all moves lose 4k” without searching the two interposes.

**Idea:** if there is a **major** capturing-range threat (path loot ≥ Hook-class, dest-empty PathClear already legal), generate or **order-boost** quiets that land strictly between attacker and loot, whose mover is in that attacker’s blocking set.

Start at **root** (maybe PV) only: empty squares on that one ray × own set-3 pieces that can legally step there. Smaller variant: don’t generate extras, just killer/history-boost matching quiets.

**Risk:** every BG aims somewhere. Gate on the same predicate as C/F (that piece just landed, ray through our Hook). Interposing with GG to save a Hook can hang the GG — still need hang/SEE.

**Check:** GUI ply 71, Black to move, BG on `11,13`, strike legal. Seed 1s depth-4 at ~35k nodes preferred `4,2-13,11`; the tournament game at ~10k played `26,12-26,25+`. C/F without J may still pick the wrong quiet.

---

## K — Deferrable conversion vs hanging take (very tentative)

**slot0202 ply 65–67.** After White VG `4,19-13,10` (pawn, lining up `13,10-3,0+` through a Flying General), Black can take the VG (Spear `13,9-13,10`, Vertical Leopard, or Capricorn) **or** Capricorn `11,16-0,27-2,25+` (Earth Chariot + promo to Hook). Promo-now is what got played.

The Hook conversion is **deferrable**: Spear takes VG → White has no dest-hit on the Capricorn at `11,16`; if White passes, `11,16-0,27-2,25+` is still legal. Capricorn itself taking the VG still has promo two-steps from `13,10` later. Take-VG-first gets ~1382 **and** still promotes, and White never fires the FG corridor.

Depth-1 greed takes the loud promo (~+3300 > VG ~1382). “VG now, Hook next” needs another Black ply.

**Not a q-rule.** Something like: if a loud promo is still legal after a **different** piece takes a hanging high-value victim, prefer the hanging take this ply. Fiddly (must not move the converting piece; must guess the opponent cannot kill the conversion in one ply). Easy to defer a promo that *is* stoppable.

If C–F see the FG laser, this position may already flip without K: promo-now hangs the corridor; take-VG-now shuts it off. Don’t try until A–F exist.

---

## M — Last-royal captures are mate / loud

**slot0151 last ply.** White Running Chariot `18,31-18,0+` takes Black’s only royal (Crown Prince on `18,0`). Black’s previous search **+759** / static **+758** — mate-in-1 missed.

Crown Prince material is **8**, below the q loud floor (~648). Taking the last royal is ordered like a pawn grab (MVV last among captures) and does not open q. White found it with ~58k nodes; Black’s previous move had ~23k.

**Idea:** a capture of the opponent’s last King/CP is mate (or at least louder than Hook). Force it first in AB/q ordering; never hang-skip it; always loud for q. Independent of L (eval safety). Small, do after A or in parallel.

---

# Eval

## L — Last-royal flight penalty

**slot0151** ends in checkmate with the loser ahead on material (seed raw ~8701 vs ~6549; Black still has 132 pieces). Tropism is **0**: density N=20 vs 131/61 non-royals, and White is behind so `eg_ahead_min` zeros White’s tropism.

Before the mate, Black’s only royal is CP `18,0` (back rank, already attacked). White King `18,35`. Both sides have one royal from ~ply 820. Neighborhood (3 squares off-board):

| sq | color | contents | White attacks |
|---|---|---|---|
| 17,0 | light | empty | Tengu `+19,10` |
| 18,1 | light | empty | Chariot + Tengu |
| 19,0 | light | empty | Tengu `+19,10` |
| 17,1 | dark | own Gold | no |
| 19,1 | dark | own Silver | no |

Three legal CP moves, all onto White-attacked lights. Dark diagonals are own generals (castle-looking). **No White piece within Chebyshev 4** — open file + promoted Tengu. Tropism would not have scored this even if gated on: chariot at d=31 is past `d_ref=18`; Tengu at d=10 would count only for the **attacker**, and only if ahead.

A raw legal-move count of 3 looks mobile; all 3 are into check. That is the own-vs-enemy split: friends on one color, no real flights on the other.

### v1

**Gate:** exactly one royal (King or CP). Skip if a spare exists. Cheap: one 8-neighborhood. No full move generation. Occupancy + `is_position_attacked_by_color` on ≤9 squares. Measure eval time.

For each king-step square:

- **flight** — empty and not attacked by the opponent
- **controlled** — empty but attacked
- **own** — occupied by us (do **not** treat like controlled)
- **enemy** — occupied by them
- **off** — off-board

Sketch:

- `in_check` = royal square attacked
- `flights >= 2` and not `in_check` → 0
- `flights == 0` and `in_check` → **large** penalty (thousands; must beat a ~2k material lead so this game still looks lost)
- `flights == 0`, not in check, many `own` → small or 0 (smothered-looking castle; jump-mates later)
- in between: scale with `(2 − flights)` more when in check

**Not in v1:** color-complex / “control the dark squares.” Here the deadly squares were the **opposite** color from the CP; the same-color diagonals were friends.

### Tropism (later; don’t overload L)

Tropism is an **attacker** term: our pieces near *their* royals, gated on **our** material lead and **their** piece-count (`eg_ahead_min`, `eg_density_n`). It does not tell a trapped king to run, and it would not have helped White here.

Possible later knobs, separate from L:

- When the **opponent** has one royal, drop `eg_ahead_min` and/or loosen density so a material-behind attacker still hunts (Tengu at d=10 would start to count).
- Keep capturing-range tropism at 0; file lasers belong to L / checks, not Chebyshev.
- Do not attract our own king toward theirs (already skipped).

L first. Tropism retune only if L makes sole-royal games too cowardly (king runs to the corner and never converts).

Royal bonus today is only “keep a spare” (`royal_bonus_by_count`: 1→0, 2→100). It does not say the remaining one is unsafe.

---

## How to measure

1. Replay the check ply, search with seed (noise 0) at depth 1, 2, and 1s depth 8. Compare best move and the hanging line’s root score to the notes above.
2. q-nodes / unique-q vs current PathAware on: a quiet opening, a GG PathClear leaf, and the hang position.
3. One letter per patch. A then B, not both.
4. Optional split for A/B: open-q-only vs also expanding those moves as q candidates.

`collect_trace` / `root_lines` on `SearchResult` is enough for “did this root move’s score fall off a cliff.”
