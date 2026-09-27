# Combined engine-speed result, 2026-09-27

The final retained branch at `7fed1be` was compared directly with the
original `198b2c0` baseline. It combines cached/global attack flags, stack
candidate bitset, lazy progress labels, state-equivalent two-step route
deduplication, bounded Free Eagle reach checks, stage-B filtering, candidate
region masks, per-color board occupancy masks and the board attack prefilter.

The existing 12-position/four-agent corpus ran at fixed depths on CPU 2,
with warmups and four alternating paired repetitions. All **192 measured
searches** returned the same complete best move, score and completed depth.
The intended deduplication changed legal-move counts in 112 pairs and main
node counts in 152. Summed main nodes fell **3,211,208 → 2,667,200**
(−16.9%); q-nodes fell **1,570,048 → 1,393,072** (−11.3%). The
candidate/baseline wall-time geometric mean was **0.7781** (22.2% less), and
process CPU-time geometric mean was **0.7773**. Summed process CPU time fell
35.268 → 27.460 s.

This is the net gain for the tested corpus and hardware, not the product of
the individual experiment ratios. The fixed-depth results do not establish
playing strength under a clock. Two-step deduplication changes move indices
and thus can change selective-search choices in untested positions. The
separate soft-stop prototype was not retained.

[Build and corpus plan](results/combined-20260927/plan.json),
[raw paired records](results/combined-20260927/pairs.jsonl.gz), and
[summary](results/combined-20260927/summary.json) are saved here. Reproduce
with `run_dedup.py` and the local corpus described in the main README.
