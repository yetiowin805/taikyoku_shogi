# Two-step route deduplication, 2026-09-27

This change builds on the exact attack-candidate/progress-label patch at
`a3d0bf8`. It removes redundant two-step routes during ordinary legal move
generation. An intermediate capture remains distinct. For an empty
intermediate, the key is destination, promotion choice and whether the move
resets the progress-draw counter. These determine the board, side to move,
draw count, position hash and repetition history after the move. The first
route in generation order is kept; stage B also avoids revisiting a plain
first-leg move already searched in stage A.

The sparse-fixture unit test covers Hook Mover, Tengu, Capricorn and Peacock
with a capture target and a near-limit progress counter. For every omitted
raw route it checks that a retained move reaches the same board piece lists,
side to move, draw counter, Zobrist hash and repetition history. It also
requires routes that capture on the intermediate square to remain present.
The full debug and release library suites each passed 387 tests with four
ignored.

## Paired search result

The existing 12-position/four-agent fixed-depth corpus was tested with two
alternating repetitions on CPU 2. The baseline was the exact speed patch at
`a3d0bf8`; the candidate added only deduplication. All **96 paired searches**
returned the same complete best move, score and completed depth. Legal-move
counts changed in 56 pairs; main-node counts changed in 76, which is the
intended reduction. Summed main nodes fell **1,605,604 → 1,333,600 (−16.9%)**;
q-nodes fell **785,024 → 696,536 (−11.3%)**. Paired process CPU-time geometric
mean was **0.9354** (6.5% less); summed process CPU time was 16.835 → 16.032 s.
Results varied by position: positions without duplicate routes had a small
overhead, while the most affected position used 46.5% as many main nodes.

The paired measurements are recorded above. Run the `run_dedup.py` script with
the local corpus described in the earlier README to repeat the comparison.

The result establishes state equivalence for the tested fixtures and unchanged
best move/score in this fixed-depth corpus. Removing routes changes move indices
and may alter LMR or other selective-search choices in other positions and
time controls. A playing-strength test remains useful before merging. Under
`AGENTS.md`, the eventual merge needs a `kind: logic` history snapshot of the
parent of that merge.
