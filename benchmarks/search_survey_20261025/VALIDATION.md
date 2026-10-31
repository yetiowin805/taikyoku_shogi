# Validation of the search survey

This branch changes only the report and its provenance. No production source, model or runtime configuration changes. Details and command output summaries are in [validation.json](validation.json).

The required local pre-push checks passed on 25 October 2026:

- `cargo fmt`: passed; no source changes.
- `cargo clippy --offline --all-targets --all-features -- -D warnings`: passed.
- `cargo test --offline`: 451 passed in total, including 440 library tests; four library tests ignored, zero failures.
- `ruff format .`: 149 files unchanged.
- `ruff check --fix .`: all checks passed; no source changes.
- `python3 -m unittest discover -s deploy -p 'test_*.py'`: 95 passed. SQLite ResourceWarnings were emitted by the suite.

Local Rust/Cargo were 1.92.0; Ruff was the repository-pinned 0.16.8, installed only in `/tmp`. CI pins Rust 1.97, so these local results are not a statement that remote CI has passed. Cargo builds used a separate temporary target directory and two build jobs. No performance measurements overlapped these checks because no performance experiments were run.

All report reference definitions resolve. Reviewed local and external file hashes match the manifest, and source citation line ranges were checked. The catalog contains 14 quiescence, 20 normal-search and 12 execution/memory options. Arithmetic examples were recomputed and are explicitly illustrative.

The local companion canvas passed strict TypeScript checking against the public `cursor/canvas` SDK declarations (`skipLibCheck` for SDK dependencies). The app queued its opening; no visual runtime verification is claimed. The complete durable report is README.md and does not depend on that local canvas.
