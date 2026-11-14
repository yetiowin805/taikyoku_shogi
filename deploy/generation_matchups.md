# Generation matchup eligibility

The ordinary top-five retirement rule and generation representatives are unchanged.
The adaptive scheduler also permits a retired agent to play ordinary exploratory
pairs against a generation represented among the current five highest published
ratings. This is conditional opponent eligibility, not a new game type.

An agent needs four completed colour-swapped opening pairs against three distinct
opponents in the target generation. Games and comparisons must be within the same
strongly connected component. Incomplete pairs do not meet the exposure floor.
All completed games in that component contribute to the matchup estimate, with
draws worth half a point. Generation membership follows entrant metadata and the
existing legacy aliases; the rule is not specific to v6 or NNUE agents.

The scheduling-only screen fits a generation matchup offset to the current
strength/color model, with a zero-centred 100-Elo Gaussian prior. A scalar Newton
fit and Laplace variance approximate the conditional offset posterior; uncertainty
in the agent/benchmark strength contrast is also included. This shares the
existing champion-counter approximation: baseline ratings reuse these results,
opponent uncertainties are not jointly refitted, and probabilities should not be
interpreted as calibrated sequential error rates or published rating estimates.

The target is performance within 50 Elo of the generation's highest-rated member.
At 25% probability, coverage-qualified retired agents become available to the
ordinary exploratory opponent selector against that generation's three highest
rated other members. Selection prioritizes a proxy for reducing uncertainty about
competitiveness. Existing pending pairings are avoided. These choices use the
existing `probe_residual` reason and normal game/result format.

Conditional pairings consume the existing 10% exploration allocation and are
additionally capped at one per twenty total pairs scheduled since this feature
was installed. No unused budget from historical games is inherited. At 90%
probability, and after four additional completed pairs since first eligibility,
the agent receives `generation_matchup:<generation>` retirement protection.
Those additional games can come from any ordinary scheduling path: there are no
fixed confirmation batches, frozen opponent panels, trial slot lists, special
rating weights, or historical game reclassification. Eligibility and protection
are recalculated as real results arrive; pending games never supply evidence.

The saved scheduler state records eligibility coverage baselines, current matchup
reports, and budget counters. Reloading identical evidence cannot create fresh
confirmation. Losing relevance, coverage, or the probability threshold removes
the corresponding exception. Existing committed return legs remain honoured.
Unknown/new generations provide no evidence and cannot trigger revival.

`scheduler_audit STATE.json` includes `generation_matchups` in its read-only
report. Inspect `reports` together with `inactive` to distinguish a retired agent
eligible for conditional pairings from an already active agent. The audit creates
only hypothetical claims in memory; it does not start or alter tournament games.
