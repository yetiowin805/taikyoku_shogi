# Conditional training teachers

`conditional-512v4-v1` fixes the primary teacher to the frozen `NNUE_W512_v4`
checkpoint. It replaces the mandatory 512v2 teacher and automatic champion
promotion. Position sampling, CPU allocation and the 30-second search budget
remain unchanged; the existing handcrafted-win sampler can still propose
positions, but its teacher hint no longer overrides these rules.

For the position before move P, use the moving agent's saved evaluation at P.
Estimate the other agent's view by averaging its saved evaluations at P−1 and
P+1. All three saved scores are black-absolute. Missing neighbors, missing scores,
or unexpected colors mean primary-only analysis. No one-sided extrapolation.

A decisive game adds its winner when either:

- Black's estimate is positive and White's is negative (each favors itself).
- Both estimates favor the winner and the winner's estimated advantage magnitude
  is at least 1.2 times the loser's estimate.

A drawn game adds the agent with the smaller absolute estimate when both
estimates have the same strictly positive or strictly negative sign. Equal
magnitudes select Black deterministically. Zero means no agreed advantage.
Thresholds apply to recorded numeric scores, including encoded mate scores;
the 20% comparison is not a comparison of calibrated win probabilities.

The secondary uses the original game's frozen model and historical engine
binding, when present. A secondary identical to the primary is searched once.
The dataset builder continues to average teacher probabilities equally. The
selection reason, both estimates, and selected side are saved in each moment's
`teacher_selection` field. Superseded labels remain in
`previous_teacher_searches`; only current `searches` contribute to new exports.
Existing exported datasets are immutable and are not modified.

## Activation

Stop only the analysis collector and let its children exit; leave game workers
running. With the collector lock released, run from the deployed repository:

```sh
python3 deploy/dual_label_sidecar.py enable-conditional \
  --run-dir /absolute/path/to/run \
  --model /absolute/path/to/NNUE_W512_v4.json
```

Then resume the collector using its existing supervisor. Activation validates the
model, backs up the catalogue/config, pins the teacher, and requeues existing
moments once. This reclassifies the existing catalogue under the new rules; it
retains the search cache, whose keys include model, helper, position and budget.
Completed labels for the same model, position, budget and explicit historical
engine binding are retained with their original engine provenance, even across
rolling updates. Missing labels use the full content-addressed search cache or
run a new search. Each completed teacher result is persisted immediately. A
restart repeats neither the migration nor completed cached searches.

The legacy path remains for existing configurations until activation. Deploying
files alone does not activate this policy. Do not invoke `replace-teacher` to
change the primary while this policy is active.
