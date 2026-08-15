# Agent notes

## Git branches

**Always start work on a new branch off up-to-date `main`.** Do not pile unrelated commits onto an old feature branch (e.g. reusing `cursor/random-worker-seeds` for hang-prune or Texel changes).

1. `git fetch origin && git checkout main && git pull --ff-only`
2. `git checkout -b cursor/<short-topic>` (or the repo’s usual prefix)
3. Commit only that topic’s changes; open a PR into `main`
4. After merge, delete/abandon the branch — next task gets a **new** branch from `main` again

Exception: continuing the *same* unfinished PR/topic on its existing branch is fine. Stacking a different feature on top is not.

## Historical eval / search snapshots

When merging a **major eval or search behavior change** (new term, eligibility, tropism/PST formula, ID/qsearch/ordering that changes move choice under the same time/depth — not a clear bugfix), add a `kind: logic` entry to [`models/history/manifest.json`](models/history/manifest.json) pointing at the **parent of the merge**, then run `./deploy/freeze_history.sh`. Weight-only bakes go in `kind: weights` (git rev of `models/ab-seed.json`).

Do **not** freeze pairing/Swiss, deploy scripts, crash fixes, notation-only, or tests. Binaries are a cache under `models/history/bin/` (gitignored); git + the manifest are the source of truth.
