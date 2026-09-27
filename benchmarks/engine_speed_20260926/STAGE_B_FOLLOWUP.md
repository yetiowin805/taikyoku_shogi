# Stage-B multi-leg filter, 2026-09-27

This D2.6 experiment builds on the Free Eagle shortcut at `426e705`.
`QuietMultiLegOnly` generation now skips a piece before boxed-in and movement
checks when its type/promotion configuration has no TwoStep capability and it
is not a Free Eagle. Base-type variants use the dynamic configuration path.
A parity test compares the filter with `MovementConfig::for_piece` for all
declared piece types, both colors, promotion states and base-type variants.

The existing 12-position/four-agent corpus matched complete search signatures
in 48 warmups and **96 measured pairs**. The candidate/baseline wall-time
geometric mean was **0.9943** (about 0.6% less). Position and agent results
were mixed, so this screen is inconclusive for whole-search speed. The filter
is retained as an exact bounded reduction in stage-B work.

The paired measurements are recorded above. Use this directory's `run.py` with
the local corpus to repeat them.
