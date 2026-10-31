# V4 / v4.5 tactical corpus — resumption guide

Start with [the 32-case report](validation/report.txt), or open
[the HTML report](validation/report.html) in a browser. The structured report is
[report.json](validation/report.json); editorial conclusions are also kept in
[report-notes.json](validation/report-notes.json).

## What is saved

The frozen source snapshot is **2026-10-10 16:20 PDT**, with 842 completed games.
All 15 v4/v4.5 losses to handcrafted or v2 opponents were mined: 294 unique
candidate cases, 1,751 position snapshots, 32 individually reviewed cases.

The report records 108 follow-up searches and 238 successfully replayed lines at
34 positions. Its classifications are:

- 1 previously isolated Capricorn search failure (817, ply 43).
- 4 cases with supported defenses that still need stronger adjudication.
- 20 verified tactical patterns with unresolved defenses.
- 6 useful controls for compensated trades or forcing moves.
- 1 unresolved score-drop case, excluded from tactical pass/fail scoring.

**These are not 32 solved tactical puzzles.** Own-model score improvements do not
prove stronger play. Compare equal completed depths, then test equal time/node
budgets. Requested depth was completed in 90 of the 108 searches.

## Restore the evidence

`evidence.tar.gz` is a 2.6 MB compressed archive containing the complete corpus,
position snapshots, trajectories, replay events, raw search logs, plans, report
sources, diagnostic patch, and **all 15 original GUI game records**. It excludes
rebuildable executables and model weights. All 504 members were read back and
checked against [evidence-manifest.json](evidence-manifest.json).

From the repository root, extract with:

```sh
tar -xzf benchmarks/nnue_tactics/v4-loss-corpus-20261010/evidence.tar.gz
python3 -m http.server 18427 --bind 127.0.0.1 \
  --directory benchmarks/nnue_tactics/v4-loss-corpus-20261010
```

Then open `http://127.0.0.1:18427/validation/report.html`. Original games restore
under `data/raw/games/nnue-v4.5-new-games/`. Extraction restores frozen versions
of generated report files, so preserve any later edits before extracting again.

The search plans retain original absolute paths for provenance. On another
checkout or machine, update the runner's `root` and the plan paths before
rerunning. The original NNUE model files are required; model/network hashes are
recorded in the original probe results. They are not bundled here.

The historical diagnostic analyzer comes from revision
`63de99c429402588de6132e4cefec3a972203fdd` plus `diagnostic.patch` inside the
archive. The patch was reapplied to that revision and verified to reconstruct
the exact three source files used for these searches. Build it in a separate
historical checkout, not by patching the playing engine. Set `TACTICAL_ANALYZER`
to the resulting `analyze_position` executable when using the saved runners.

The current `tactical_line` helper is built with
`cargo build --release --bin tactical_line`. Its API and limitations are in
[`deploy/tactical-corpus/README.md`](../../../deploy/tactical-corpus/README.md).
Replay `validation/line-plan.json` to regenerate line checks. The report renderer
uses those checks and saved search results; it also hashes the diagnostic
executables at their recorded local paths, which must be updated if relocated.

## Pick up here

1. Investigate quiet defenses and capture of the attacker at **673/174,
   756/396, 869/25**, alongside established diagnostic **817/43**. At **869/329**,
   a supported alternative avoids an unsafe promotion but royal pressure also
   needs adjudication.
2. Preserve the checks and compensation controls. In particular **756/332** is
   a checking intermediate move, not merely a bad small-piece exchange.
3. Test the distinctive **864/484** threat: a Lion Hawk can capture both remaining
   royals in one compound move even though the single-last-royal check flag is
   false. Any accepted defense must preserve royal survival.
4. Establish accepted defensive moves or tactical invariants using stronger
   opponent replies. Do not treat a move change or a larger own-model score as
   proof. Investigate quiescence eligibility, reductions and ordering separately.
5. Keep whole games **672, 769 and 867** reserved. They were mined, but were not
   included in this 32-case review or its searches. Shared openings and related
   episodes within a game are correlated.

The live update at **2026-10-10 18:09 PDT** reached 849 completed games. Newly
finished losses **868** (512v4.5 vs C2K100A1D50R) and **870** (384v4.5 vs A80_L0)
qualify for a future expansion; they are not part of this frozen corpus. Slot
881 is also a new 512v4 loss, but its 384v3 opponent is outside the current
handcrafted/v2 selection scope.

No playing-engine search or evaluation behavior has been changed by this PR.
