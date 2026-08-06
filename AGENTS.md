# Agent notes

## Git branches

**Always start work on a new branch off up-to-date `main`.** Do not pile unrelated commits onto an old feature branch (e.g. reusing `cursor/random-worker-seeds` for hang-prune or Texel changes).

1. `git fetch origin && git checkout main && git pull --ff-only`
2. `git checkout -b cursor/<short-topic>` (or the repo’s usual prefix)
3. Commit only that topic’s changes; open a PR into `main`
4. After merge, delete/abandon the branch — next task gets a **new** branch from `main` again

Exception: continuing the *same* unfinished PR/topic on its existing branch is fine. Stacking a different feature on top is not.
