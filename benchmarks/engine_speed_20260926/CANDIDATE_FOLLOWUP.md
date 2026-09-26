# Candidate scan follow-up, 2026-09-26

Base revision: `198b2c0` (`origin/main` when the work began). The retained patch
caches `needs_global_scan` for ordinary piece types and promotion states, keeps
the legacy classifier for `base_piece_type` variants, replaces the per-query
candidate bitset allocation with 21 stack words (enough for 1,296 board slots),
and delays root/quiescence move-label formatting until a three-second progress
line is emitted. It does not change move order, eligibility, evaluation or time
management. The final debug and release library suites both passed (386 tests,
four ignored). One intermediate debug invocation hit a timing-sensitive royal
probe assertion; the repeat passed. The new cached-classifier parity test covers every declared
piece type, both colors, both promotion states and base-type variants.

## Final paired check

The existing `search_speed_20260920` JSONL harness loaded the same 12-position,
four-agent local corpus for both release builds. It pins the searches to CPU 2,
warms every case, balances execution order and compares the complete search
signature (route, score, main/q nodes, depth, static evaluation and ordered root
lines). All **48 warmups and 48 measured pairs matched exactly**. The
candidate/baseline wall-time geometric mean was **0.9380**; by agent, the ratios
were 0.9200, 0.9250, 0.9479 and 0.9599. All 12 position aggregates were below
one, ranging from 0.8975 to 0.9866. This is a short paired speed screen; it does
not measure playing strength or guarantee the same gain on other hardware.

[Plan and build identities](results/candidate-20260926/plan.json),
[paired records](results/candidate-20260926/pairs.jsonl.gz), and
[summary](results/candidate-20260926/summary.json) are saved here. The corpus
and model files are local and are described in `../search_speed_20260920/README.md`.
To reproduce, build `../search_speed_20260920/harness.rs` as example
`speed_experiment` on `198b2c0` and this branch, then run this directory's
`run.py` with the local `corpus.json` and `cases.json` as in the main README.

## Explored alternatives

- Scanning the whole army directly instead of the window/rays was 5.3% slower
  in an earlier 96-pair exact screen. Adding a cached exotic flag and stack
  bitset made that direct scan faster than baseline, but it was only 0.7% faster
  than the original window/ray scan with the same cache and stack bitset.
  The simpler retained change keeps window/ray candidate selection.
- Caching royal counts passed parity but was approximately 0.4% faster in a
  mixed screen. The extra state did not justify retention.
- Lazy label formatting was approximately 0.6% faster in an earlier 96-pair
  incremental screen; that isolated result is near timing noise. It is retained
  because it removes formatting that almost never reaches the log.
- A B8 trace of 72 timed searches found about 58–60% of the budget elapsed
  after the last completed depth. A conservative counterfactual stop after
  depth two saved an estimated 10% (1-second budgets) or 22% (3-second budgets)
  of clock with the same returned moves in the trace. An actual timed
  soft-stop comparison was interrupted before its output could be retained.
  No soft stop is included in the final patch.
- A D2.2 two-step-route dedup prototype cut a synthetic sparse position's
  legal moves from 3,646 to 1,926 and depth-two nodes from 45,998 to 27,065,
  with the same score and chosen route. On 48 existing corpus/agent cases,
  28 root legal counts and 38 node signatures changed; all 48 chosen routes
  and scores remained the same. This needs progress-draw state parity and a
  strength tournament before merging; the route change would require a
  `kind: logic` history snapshot under `AGENTS.md`.

The exploratory screens above were completed before a temporary experiment
checkout was removed during a session transition; their raw files are not part
of this branch. The final paired check was rerun in a persistent checkout and
its raw outputs are included.
