# Conservative ID soft stop, 2026-09-27

## Implementation after the trial

The measured rule is now enabled for timed searches after completing depth two.
Before starting the next depth, search returns the last completed result when
twice the preceding iteration's duration exceeds the time left before the hard
deadline. Untimed searches still run to their requested depth. The original
trial and its limitations are recorded below; the trial was conducted before
this implementation and is not a game-level strength result.

A fresh comparison of this branch against merged `main` used the same 48
one-second and 24 three-second corpus cases. At one second, chosen moves
matched in 48/48 cases, completed depths in 44/48, and summed search time
fell 48.02 to 38.29 seconds (20.3%). At three seconds, chosen moves matched
in 23/24 cases, completed depths in 23/24, and summed search time fell 72.02
to 68.35 seconds (5.1%). In the differing three-second case, this branch
stopped at depth two while baseline completed depth three. Thus the clock
savings replicate, but move parity does not hold universally even on this
corpus. A game-level strength measurement has not been done.

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
different moves elsewhere. At the time of this trial, the prototype remained
unmerged pending a game-level comparison or a safer adaptive rule.

The timed measurements are recorded above. The `run_timed.py` script can repeat
the trial with the local search-speed corpus after applying the soft-stop rule.
