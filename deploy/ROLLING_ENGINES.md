# Update tournament engines between games

Rolling mode keeps the tournament coordinator and bracket state alive. Each
worker claims its normal CPU lease, reads `analysis/active-engine.json` once,
and launches that immutable executable for one whole game. Both ordinary agents
use that build; explicitly historical agents still use their pinned think-loop
executables. Game execution, move validation and evaluation all run in the game
process, not in the old coordinator.

## Enable once

Build/install this PR's coordinator and analysis tooling. An already-running
pre-PR coordinator cannot acquire this capability dynamically: stop and resume
it once with the existing launcher and `--rolling-engines`:

```sh
python3 deploy/tourney_analysis.py stop --run-dir "$RUN"
cargo build --locked --release --bin taikyoku_shogi --bin analyze_position
python3 deploy/tourney_analysis.py resume --run-dir "$RUN" --rolling-engines
```

The existing stop operation aborts in-flight games; resume requeues those games
from their starts. Completed results and brackets are preserved. Future resumes
retain rolling mode automatically. New runs can pass `--rolling-engines` to
`start`. Normal non-rolling launches remain supported.

## Publish a compatible speedup

After merging and validating a behavior-preserving change:

```sh
./deploy/update_tourney_engine.sh "$RUN" --compatible-speedup origin/main
```

This fetches the requested revision, builds only the game executable and its
analysis helper in an isolated temporary worktree with one build job, validates
both binaries against every non-historical model, and publishes an atomic build
selection. It never overwrites an executable a running game is using. Build or
validation failure leaves the selection unchanged. The installed deployment
script is used, rather than deployment code from the selected revision.

For an already-built pair:

```sh
python3 deploy/tourney_analysis.py update-engine --run-dir "$RUN" \
  --engine /absolute/build/taikyoku_shogi \
  --analyzer /absolute/build/analyze_position \
  --revision COMMIT_OR_BUILD_DESCRIPTION --compatible-speedup
```

`--compatible-speedup` is an operator assertion, not an automated proof. Use a
new run/entrant identity for search-policy, evaluation, rule, or checkpoint
changes. This mechanism updates game executables and analysis helpers; it does
not hot-reload coordinator/deployment code or change model files.

Workers already playing finish with their selected build. Idle workers take the
new selection on their next game. The fourth worker continues to obey the shared
CPU lease; the coordinator does no search and does not create an extra compute
worker. Children inherit the worker thread's CPU affinity. Stop/crash cleanup
kills game process groups, and Linux parent-death guards also cover historical
think-loop children. A failed game is not rated; admission stops, other games
finish, and the supervisor reports failure. Fix/roll back and resume to retry
aborted slots. Interrupted games retain the existing restart-from-start behavior.

## Status and rollback

```sh
python3 deploy/tourney_analysis.py status --run-dir "$RUN"
python3 deploy/tourney_analysis.py update-engine --run-dir "$RUN" \
  --rollback-build SAVED_64_CHARACTER_BUILD_ID
```

Status includes the selected build ID and the builds of active games. Saved
build manifests live in `analysis/engine-builds/`. Rollback validates the saved
pair and current models before selecting it for future games; running games
continue unchanged. Old binaries are intentionally retained for game provenance,
analysis and rollback; do not garbage-collect them while records reference them.

Each side's game record includes `engine_sha256`; ordinary sides also include
`engine_build` with revision, binary hashes and matching helper paths. Older
records still deserialize without these fields. Non-teacher analysis follows
the game's original bundle; legacy records retain the baseline helper. Training
label analysis selects the latest bundle at each request boundary. Its cache key
already includes the helper hash, so different versions cannot reuse each
other's search results. Existing completed catalogue entries remain valid and
are not reanalyzed automatically. An in-flight analysis request keeps its original
helper even if publication occurs while it searches.

The binaries and records must stay within a trusted run directory. SHA-256 checks
detect missing/changed snapshots; this is not a sandbox for untrusted executables.

## Validation

`cargo test --test rolling_game_process -- --test-threads=1` exercises a live game
across publication, next-game uptake, rollback, historical pinning, stopped and
crashed children, stale-result rejection, and a real separate-process game versus
the in-process result. `python3 -m unittest discover -s deploy -p 'test_*.py'`
covers launcher/model/cache behavior and existing analyzer/CPU-sharing recovery.
