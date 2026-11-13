# Local tournament dashboard and generation estimates

This directory versions the presentation and its analysis integration. Live
snapshots, posterior draws and HTML bundles remain in the operator's local data
and web directories. Nothing here changes tournament scheduling, retirement,
NNUE evaluation or the position analyzer.

## Build the dashboard

Install the Node dependencies in this directory (`npm install`), then run:

```sh
node deploy/dashboard/build.cjs /absolute/path/to/existing/dashboard
```

Alternatively set `DASHBOARD_NODE_MODULES` to an installed dependency directory.
The builder reads the three existing payloads from `TournamentApp.tsx`, validates
them and retains the exact data anchors consumed by `update_preview.py`. It
backs up the previous dashboard and statistical study before the first build.
The old launch proposal and SEEDS2 evidence remain in that archive, unchanged.
The retirement renderer still owns its table; the navigation shows it only in
Retirement. Historical brackets and the active-only sensitivity are in Archive.
The old retirement URL fragment still works. Rebuilding does not download data.

## Statistical environment

Use a separate persistent environment (the frozen requirements were exercised
with Python 3.13). The main deployment runtime does not need JAX or NumPyro:

```sh
python3 -m venv data/generation-strength-venv
data/generation-strength-venv/bin/python -m pip install -r deploy/dashboard/analysis-requirements.txt
```

`deploy/generation_strength.py fit --snapshot SNAPSHOT --output PREFIX` creates
posterior samples and diagnostics. The input is a JSON object containing the
same `state` and `fit` from one dashboard capture. Generation membership comes
from `fit.retirement.generations`; known network parents and search recipes are
inferred from registered descriptor names. Unrecognized parent relationships
remain ordinary generation members. No generation labels or particular agent
are required by the fitter. Retrospective validation separates within-generation and between-generation
games without privileging a named cohort.

Strength is centered at 1500 across **all registered agents** in every draw,
including inactive agents. This is a display convention, distinct from the
published rating scale. Pairwise differences are invariant to this centering.
The expected best of k agents averages over uniformly chosen distinct subsets
of each existing roster. The common k is min(5, smallest generation size).

## Automatic refresh

Initialize an update with a previously converged structured fit, or omit the
baseline arguments to start a new fit:

```sh
data/generation-strength-venv/bin/python deploy/generation_strength_refresh.py request \
  --snapshot CAPTURE.json --work-dir data/analysis/generation-strength-live \
  --web-dir /absolute/path/to/dashboard \
  --baseline /absolute/path/to/structured \
  --baseline-snapshot /absolute/path/to/structured-snapshot.json
```

Install the refresh hook once:

```sh
python3 deploy/dashboard/install_refresh_hook.py \
  --web-dir /absolute/path/to/dashboard \
  --work-dir /absolute/path/to/analysis-work \
  --python /absolute/path/to/analysis-venv/bin/python
```

Every normal dashboard refresh queues its new evidence. There is no extra VPS
poller. One local, low-priority worker uses at most two allowed CPUs and coalesces
new requests while working. The report polls local accepted results/status every
30 seconds. It preserves the previous accepted result while pending or failed.
A restart/crash is detected on the next dashboard refresh; the worker log and
requested snapshot remain in the work directory.

For unchanged rosters, the update uses Pareto-smoothed importance weights from
the last full fit, never a chain of previously weighted approximations. It
recomputes joint likelihoods for complete affected opening blocks, including
partial color-return pairs and corrected/deleted results. It accepts Pareto k
below 0.7 and weight ESS at least max(400, 10% of original draws). These are
numerical gates, not a guarantee that the statistical model is correct.

Roster/design changes or failed weight diagnostics trigger a fresh 4-chain fit
(600 warmup + 1,000 samples per chain, 15-point opening-effect quadrature). It is
published only with no divergences/depth-limit hits, monitored R-hat ≤1.05 and
ESS ≥200. A full fit can take tens of minutes on a laptop. A failed convergence
gate leaves the last result visible and reports failure; the next dashboard
refresh retries. Historical validation/prior sensitivity is archived, not
silently relabeled as validation of later snapshots.

Optional numerical tests:

```sh
PYTHONPATH=deploy data/generation-strength-venv/bin/python -m unittest \
  deploy.test_generation_strength deploy.test_generation_strength_refresh
```

The default deployment test suite skips numerical tests when these optional
libraries are absent. All file/queue and evidence-identity tests still run.

After building, `node deploy/dashboard/check.cjs WEB_DIRECTORY` exercises
navigation, archived evidence retention, retirement visibility and live report
updates in JSDOM. It performs no network calls and takes no screenshots.
