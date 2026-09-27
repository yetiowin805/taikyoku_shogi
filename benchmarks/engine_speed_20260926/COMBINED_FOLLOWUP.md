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

The paired measurements are recorded above. Reproduce with `run_dedup.py` and
the local corpus described in the main README.

## Clock-matched follow-up

The same two release builds also ran a depth-eight timed trial: 48 paired
one-second cases and 24 paired three-second cases. Both used approximately
their full allotted clock. At one second, the final engine completed one
additional depth in **6/48** cases and chose a different move in **2/48**.
At three seconds, it completed one additional depth in **3/24** cases and
chose a different move in **1/24**. Scores changed in six and three cases,
respectively. The extra depth is an observed consequence of higher throughput;
this trial does not determine whether the changed moves are stronger.

Reproduce the clock-matched comparison with `run_timed.py` and the same local
corpus.
