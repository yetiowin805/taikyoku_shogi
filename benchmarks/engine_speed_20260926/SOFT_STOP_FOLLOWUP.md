# Conservative ID soft stop, 2026-09-27

This B8 trial ran on top of `ccf5ecd`. After a completed iteration at depth
two or deeper, the prototype stopped when twice the just-completed iteration's
elapsed time exceeded the clock remaining before the hard deadline. It left
untimed searches unchanged. This was an actual timed-search comparison, not
a trace-only counterfactual. The prototype was removed from the working branch
after measurement.

The existing 12-position/four-agent corpus supplied 48 one-second searches.
Every other position supplied 24 three-second searches. Both variants used
the same release harness and local models on CPU 2, with fixed-depth warmups,
alternating pair order and depth cap eight.

At **one second**, all **48/48 chosen complete moves matched**. The candidate
returned the same score and completed depth in 46/48 cases; the other two
finished a depth shallower and returned different scores, but the same move.
Summed wall time fell **48.03 → 39.59 s** (17.6% less); paired geometric-mean
ratio was **0.7920**. At **three seconds**, all **24/24 moves matched**; 22/24
completed the same depth and 23/24 returned the same score. Summed wall time
fell **72.02 → 62.32 s** (13.5% less); geometric-mean ratio was **0.8268**.

The trial establishes saved clock and no move changes in these 72 cases. It
does not establish equal playing strength: the shallower searches could choose
different moves elsewhere. The prototype remains unmerged pending a game-level
comparison or a safer adaptive rule.

The timed measurements are recorded above. The `run_timed.py` script can repeat
the trial with the local search-speed corpus after applying the soft-stop rule.
