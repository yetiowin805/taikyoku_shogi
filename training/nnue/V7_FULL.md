# Full v7 campaign

`v7_full.py` prepares a frozen campaign and supervises migration, collection and
training in order. It does not admit models to the tournament. Playing evaluation
and a review of the untouched test set follow training.

## Inputs and migration

Schema 2 retains shared movement channels and adds a square-bound current-piece
identity and remaining promotion availability. Behavior variants of Whale and
RainDragon have distinct identities. It also corrects Flying Crocodile's encoded
directions and White jump reflection. Schema 1 remains loadable and unchanged.
Container magic remains `TKNNUE01`; the header's feature version distinguishes
the schemas. Tools default to schema 1 unless schema 2 is explicitly requested.

Parents are the current active residual leaders at widths 256 and 384, selected
using the existing order-neutral standards. Migration copies compatible movement
rows, feature biases, dense weights and fixed-material settings. New identity and
promotion rows begin at zero. Ambiguous legacy jump/knight rows are reset rather
than copied. Overall evaluations need not match the old parent after correcting
its input semantics. A common four-window A prelude at each width learns the new
inputs before the treatments fork. This prelude does not remove material.

Schema-2 collection records the legacy position fingerprint as well as its new
fingerprint, so schema changes cannot bypass inherited exposure checks.

## Shared collection

- Freeze the current completed-game pool; reject aborted or invalid games.
- Preserve inherited train/validation/test membership and related-game groups.
  Include the earlier pilot's exposure manifest; its validation is development
  data, not a new independent test.
- Among wholly fresh eligible groups, reserve the newest 10% by recorded game
  timestamp for test and a deterministic random 10% for validation. Require at
  least 100 fresh groups. Never use test targets for checkpoint selection.
- Allocate eight representative and two challenge roots per selected game,
  capped at 40 roots per related group. Select up to four games in larger groups
  deterministically, rather than allowing long or repeated families to dominate.
- Share teacher work across S1/S2/S3. Audit 10% of roots and deepen 20% of those
  audits. Training audits mix random and targeted selection; holdout audits are
  random. Existing teacher selection, disagreement checks and quality gates apply.
- S2 settles roots. S3 explores four deterministically selected roots per related
  group, with three-ply continuations and settling before labeling endpoints.

Training masses are inherited from the pilot:

- S1: 75% representative, 20% challenge, 5% alternative.
- S2: 25% raw anchors, 50% settled representative, 20% settled challenge,
  5% settled alternative.
- S3: 50% representative, 10% challenge, 40% alternative.

Missing categories redistribute their mass. Dataset views share immutable feature
storage; they do not repeat the expensive teacher searches.

## Training

The matrix is 256/384 × S1/S2/S3 × A/Ao/C/Cu/E: 30 exported networks, plus the two
common preludes. All models retain fixed material. A keeps its 15% outcome blend;
Ao keeps the deliberately larger 30% outcome contrast. C, Cu and E retain their
pilot objectives and uncertainty/adaptive weighting behavior.

Each window has 65,536 weighted draws and effective batches of eight. Main runs
have at least eight windows, validation-based checkpoint selection, plateau
stopping and two learning-rate reductions. There is no fixed window or elapsed
time ceiling. E begins its training-only adaptive updates after four windows.
Common validation predictions are recorded at windows 4, 8 and every eight
windows thereafter. The pilot's matched-budget stopping rule does not carry over.

## Operations and storage

Run compilation, corpus processing, training and parity checks on the VPS. Keep
the laptop to bounded text edits and status reads. The collection requests up to
six flexible scheduler slots; protected game workers 0 and 2 remain available.
Training concurrency defaults to one and can be two only after measured memory
headroom supports it. Do not launch training before migration/export/Rust
inference checks pass at both widths.

Use the configured training virtualenv without resolving its Python symlink.
Preparation pins binary/model identities; finish source changes and freeze the
revision before preparing the production campaign. Preparation itself must run
under a scheduler lease. `run` acquires its own leases and holds a campaign lock.
Failures record a durable error and stop subsequent stages.

Schema 2 has 391 channels and 1,013,472 feature rows. The 30 final dense int16
embedding tables occupy approximately 18.1 GiB before small headers/heads. Float
training checkpoints, shared preludes, datasets, and atomic temporary generations
require additional space. Do not size storage from exported files alone.
The current Rust loader keeps dense weights in process memory: approximately
0.48/0.72 GiB per distinct 256/384 model, before search state and caches. Measure
tournament-worker memory and set safe concurrency before admitting these models.

Full training integrates the existing archive-storage policy: pin inputs and
reserve peak workspace before fitting, check filesystem headroom at window
boundaries, and archive completed optimizer checkpoints when the host policy
enables it. The archive system verifies remote readback before evicting local
copies. Exported networks and common prelude starts remain available. Interrupted
training checkpoints stay local for resumption. Archive failures are visible;
do not bypass disk admission or remove old game/model data to force a launch.

Unit tests must not inherit the production archive configuration. Point
`TAIKYOKU_ARCHIVE_CONFIG` to a nonexistent test path for hermetic regressions;
archive-specific tests construct their own policies. Production runs use the
actual host policy.
