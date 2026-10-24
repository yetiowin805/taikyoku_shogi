# VPS storage lifecycle plan

Status: planned implementation, following the verified operational recovery on
2026-10-23. This document records the work so it can resume independently of the
chat. It does not enable new cleanup jobs or change the running tournament.

## Recovery baseline and remaining work

A one-time recovery deduplicated immutable models, removed proven redundant
analysis artifacts and reinstallable download caches, and verified an off-site
archive of closed training files before removing their local copies. Original
game bytes and authoritative label databases were preserved. A separate restore
to disk also passed its original-content hash check.

Operational inventories, archive keys, host paths, and restore receipts remain
with the private archive. They must stay outside this public repository. The
archive is not a full VPS backup: current games, model history, and a consistent
label catalogue still need off-site backup coverage. Automatic hydration, cache
eviction, and recurring archive retention remain future work.

The audit identified these causes of continued growth:

- Feature shards have multiple dataset references. A completed run or old
  directory is not evidence that its features are unused.
- Snapshot sharing covers admission snapshots, while finished training exports
  and frozen parent copies can still duplicate the same weights.
- Retirement is reversible. Retired weights need reactivation hydration before
  they can leave local storage; teacher, historical-search, queued-work, and
  rollback references also need pins.
- Search results are stored in searches and embedded again in moment histories.
  The audited SQLite catalogue had no freelist pages, so VACUUM would not solve
  this growth.
- Log rotation and retention of the newest three managed teacher backups already
  exist. Legacy and incomplete backups need separate review.
- The deployed Rust coordinator removes iteration sidecars after saving their
  telemetry. The actual analysis collector was older than the repository code
  and still produced moment copies and retained committed progress. Deploy the
  existing cleanup before restarting analysis, preserving other live patches.

Compression probes support archiving, but are not capacity guarantees: 100 recent
completed game records retained about 10.6% of their original size, and twelve
sampled feature shards retained about 0.51%. Measure the full corpus and restore
cost before sizing a permanent cache around those samples. Raw game-record
storage excludes labels and experiment output.

## Implementation order and acceptance checks

### 0. Deploy the collector cleanup before restarting analysis

Deploy the existing database-only moment behavior and removal of progress files
after their search commits. Preserve the dirty live checkout's other patches,
service configuration, and a rollback copy; do not reset the checkout wholesale.
Relevant code: [`tourney_analysis.py`](tourney_analysis.py),
[`dual_label_sidecar.py`](dual_label_sidecar.py), and
[`prune_run_artifacts.py`](prune_run_artifacts.py).

Acceptance: a completed analysis search yields the same authoritative catalogue
result without a duplicate moment file or committed progress file. Interrupted,
uncommitted progress stays recoverable. Tournament workers continue normally.

### 1. Publish each immutable model once

Extend [`artifact_storage.py`](artifact_storage.py) sharing to completed training
exports and frozen parents as well as admission snapshots. Preserve stable paths,
content hashes, and model bindings. Publish a verified canonical blob and then
atomically replace the reference; never alias a mutable optimizer/training inode.

Acceptance: admission and training retries preserve hashes and bindings; failed
publication leaves the prior path valid; a successful export/admission leaves
one stored copy of each unique model. Test interrupted publication and concurrent
references before generalizing the recovery's one-time deduplication.

### 2. Add hydration, dependency pins, and a bounded feature cache

Index compressed chunks by content hash, with versioned dataset manifests.
Training preparation restores and verifies the **complete working set locally**
before starting the existing mmap loader. Reference counting must resolve paths,
symlinks, and shared shards across runs and both filesystems. Pin active and queued
training inputs, committed games/models, current teachers, historical-search
dependencies, and rollback material. Evict only unpinned, verified off-site data.

Acceptance: a cold dataset hydrates into the same bytes, sample order, offsets,
and hashes; concurrent and queued jobs cannot lose inputs. Test missing or corrupt
objects, retries, interrupted publication, and insufficient space. If the working
set exceeds the cache budget, reject the new job before allocation while games
continue, use a larger temporary training disk, or separately benchmark a new
loader. Do not silently change sampling or evict active chunks.

Add explicit hydration gates for historical GUI/replay reads and reversible model
reactivation **before** evicting their required game/model data. Archiving a backup
copy of those objects can start earlier, without removing local originals.

### 3. Store experiment variants as overlays

Keep common samples and features once; record sample IDs, weights, target
overrides, and mixture recipes per variant. Share immutable frozen game inputs
by their original byte hashes. See
[`recent_data.py`](../training/nnue/recent_data.py),
[`v5_weights.py`](../training/nnue/v5_weights.py), and
[`v5_snapshot.py`](../training/nnue/v5_snapshot.py).

The v5 corpus and weighted dataset each rewrote roughly 2 GiB of metadata. For
1,140,593 samples, a float64 weight column is about 8.7 MiB before compression;
retaining float64 can preserve the existing numerical weights.

Acceptance: exact split membership, canonical position IDs, sample order,
teacher provenance, and weights match. Expanded overlays produce equivalent
batches to the existing loader. Preserve original game bytes rather than
reserializing them: their SHA-256 identities are already dataset dependencies.

### 4. Store each label search result once

Normalize the catalogue so moments reference a result under its exact request
identity, including engine/model hashes and all search settings. Reference the
source game and ply instead of embedding another original move. Retain scores,
PVs, depths, budgets, and previous-teacher history.

Acceptance: teacher refresh, interrupted-search recovery, reconstructed moments,
and training-label queries pass parity checks. Back up a consistent database
before a transactional migration and exercise rollback. Compression alone does
not remove the duplicate logical payloads.

### 5. Bound checkpoint and backup lifecycles

During active training, retain the current resumable checkpoint and best
checkpoint, with crash-safe publication. On completion, archive the selected
resume state and its metadata. Keep the existing three managed teacher rollback
backups locally; migrate legacy backups only after verification. Preserve
incomplete archives and uncommitted work for diagnosis.

Acceptance: an interrupted write keeps the prior checkpoint; teacher rollback
and a cold checkpoint restore work. Temporary download, compression, and backup
space must be included in admission rather than discovered during publication.

### 6. Admit work against its peak space requirement

Extend [`storage_guard.py`](storage_guard.py) with a per-filesystem estimate of
missing unique inputs, archive download/decompression staging, two checkpoint
generations, model publication, and backup staging. Concurrent reservations must
prevent two individually admissible jobs from overcommitting the same space.

Target normal free space of **8 GiB on root and 10 GiB on the volume**. Preserve
the current hard reserves of **3 GiB on root and 5 GiB on the volume**. Attribute
growth separately to games, labels, features, models, and checkpoints. Pins
override quotas and age.

Acceptance: reject a new experiment before downloading or allocating when its
peak cannot fit; keep existing games running while healthy. Test concurrent
admission and release of reservations after failure. The existing guard still
halts writers if a hard reserve is reached.

## Suggested delivery batches

1. Collector deployment, export deduplication, and a general verified
   archive/index/restore command. Back up unique games, model blobs, and a
   consistent label snapshot early. Exercise failed upload/readback, retry,
   interrupted publication, and a large-object restore before local eviction.
2. Dataset hydration, dependency pins, concurrent peak-space admission, and
   bounded cache eviction. Only then move inactive features and legacy material
   after a fresh reference census.
3. Sample overlays and normalized label storage, each with parity checks and
   rollback. These reduce bytes produced by future experiments and analysis.
4. Bounded remote packs, versioned catalogue snapshots, retention, and periodic
   restore drills for current results as well as old training material.

Keep live state/current checkpoint, SQLite plus WAL, active/committed agents,
current teachers/search binaries, and active training inputs hot. Finished
feature chunks and likely-needed resume states can use a local cache. Inactive
datasets, optimizer states, old backups, and detailed traces can be archived.
Only proven redundant copies are disposable without a restore dependency.

Use the existing private object-storage account for off-site archives. Prefer
indexed compressed packs, initially around 128–256 MiB of uncompressed input,
rather than one tiny object per position. Validate pack size and restore cost in
practice. Keep remote objects until every referenced manifest and its retention
obligations permit deletion. No recurring job or deletion policy was installed
during the recovery.

## Recovery requirements for the general archive tool

The private operational archive holds the existing inventory, hashes, receipts,
and one-time restore helper. Restore missing inputs/checkpoints before resuming
an affected training recipe; loaders do not yet hydrate automatically. Existing
`.gz` checkpoints must return as the exact original `.gz`, with decompression an
explicit step when the consumer requires an uncompressed checkpoint.

Generalize the tested verification sequence without publishing private object
keys or host paths:

1. Record original content hash, byte count, encoding, dependencies, and metadata.
2. Upload an immutable object and read it back in full; check both the archive
   hash and restored-content hash before any local eviction.
3. Publish the remote index and local recovery receipt durably before removal.
4. On restore, reserve download plus decompression space above the filesystem's
   hard reserve. Verify original bytes before atomic publication and refuse to
   overwrite different existing data.
5. Exercise an actual restore to disk as well as streaming verification. Keep
   incomplete work for diagnosis and make retries safe.

Whole-object reads intermittently stalled during recovery. Bounded 64 MiB ranged
reads with a current transfer client passed a separate checkpoint restore. Keep
whole-object hash verification even when transport uses ranges; do not substitute
partial reads or object listings for readback. Test retries and large objects.
The exact client/provider/transport cause of the stalls was not established.

## Starting the next implementation session

Read this plan and the linked storage/deploy code, refresh free space and service
state using the private operations inventory, and rebuild the dependency census
before choosing eviction candidates. Preserve live patches and original game
bytes. Start with the collector version check, then the delivery batches above.
Do not treat the dated recovery as evidence that an input is still inactive.
