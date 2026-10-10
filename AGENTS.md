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

## Deploy scripts

Fail **loud**. A wrapper must exit non-zero and print on stderr what step died (pull, build, freeze, grid gen, lock) and that the tourney **did not start**. Do not detach, write a pid, or look successful if a prerequisite failed. A missing `data/run/*.log` / `*.pid` after `./deploy/pull_build_*.sh` means it never launched — do not leave the operator to discover that via `tail: no such file`. Stale git worktrees, missing manifests, and `set -e` mid-freeze are not silent skips.

## Before pushing

CI (`.github/workflows/ci.yml`) runs these; run them yourself first:

```sh
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings && cargo test
ruff format . && ruff check --fix .
python3 -m unittest discover -s deploy -p 'test_*.py'
```

Lint settings live in `Cargo.toml` (`[lints.clippy]`) and `pyproject.toml`. Fix the warning;
don't add `#[allow]` / `# noqa` unless the lint is wrong for that line, and say why in the comment.
Rust tests need full git history (`git fetch --unshallow` in a shallow clone).

## Keeping the code clean

- **Experiment flags get removed.** A new `SearchConfig` / `EvalWeights` toggle needs a comment naming
  its experiment. When the experiment is decided, delete the losing path (the history freeze keeps old
  behavior reproducible); don't keep both "just in case".
- **Don't copy-paste a variant.** No new `*_grid.rs`, `pull_build_*_swiss.sh` or `run_*_swiss.sh` cloned
  from an existing one: parametrize the shared code instead.
- **Python is normal Python:** one statement per line, one import per line (the formatter enforces this).
- **`benchmarks/`** records results and the git rev they ran at, not copies of source files.
