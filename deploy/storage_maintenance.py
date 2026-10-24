"""Back up immutable results and consistent labels; trim only verified unpinned features.

Inventory and policy remain private. No game/model eviction or remote deletion.
Run with the archive policy and storage guard installed; timers are optional.
"""

import contextlib
import json
import os
import shutil
import sqlite3
import sys
import tempfile
import time
from pathlib import Path

import archive_storage
import storage_guard


def maintain(archive):
    config = archive.config.get("maintenance", {})
    repo = Path(archive.config.get("repo", Path.cwd()))
    root = archive.root / "catalogue-snapshots"
    root.mkdir(exist_ok=True)
    # Old snapshots have a verified remote copy, but retain the newest one on
    # disk too. A new backup is only accepted after an actual readback.
    for name in config.get("catalogues", []):
        source = Path(name)
        with tempfile.TemporaryDirectory(prefix=".snapshot-", dir=root) as directory:
            target = Path(directory) / "labels.sqlite"
            with contextlib.closing(sqlite3.connect(source.as_uri() + "?mode=ro", uri=True)) as db:
                with contextlib.closing(sqlite3.connect(target)) as out:
                    db.backup(out)
                    if out.execute("PRAGMA quick_check").fetchall() != [("ok",)]:
                        raise ValueError("Invalid consistent catalogue snapshot")
            h = archive_storage.digest(target)
            published = root / (h + ".sqlite")
            target.replace(published)
            archive.archive([published], "labels")
            # All local catalogue generations are individually verified, and
            # old snapshots can be discarded once the new one is safe.
            for old in root.glob("*.sqlite"):
                if old != published:
                    with archive.locked() as db:
                        saved = db.execute("SELECT sha256 FROM files WHERE path=?", (str(old),)).fetchone()
                    if saved and archive_storage.digest(old) == saved["sha256"]:
                        old.unlink()
                        with archive.locked() as db:
                            db.execute("UPDATE files SET evicted=1 WHERE path=?", (str(old),))
    games = []
    models = []
    metadata = []
    snapshots = archive.root / "runtime-snapshots"
    snapshots.mkdir(exist_ok=True)
    for name in config.get("runs", []):
        run = Path(name)
        state = json.loads((run / "state.json").read_text())
        for source in [
            run / "state.json",
            run / "analysis/config.json",
            run / "analysis/manifest.json",
            run / "analysis/dual-label-config.json",
        ]:
            if source.exists():
                data = source.read_bytes()
                json.loads(data)
                import hashlib

                dest = snapshots / (hashlib.sha256(data).hexdigest() + ".json")
                dest.write_bytes(data)
                metadata.append(dest)
        for entrant in state["entrants"]:
            descriptor = Path(entrant["model"])
            if not descriptor.is_absolute():
                descriptor = repo / descriptor
            models.append(descriptor)
            binding = json.loads(descriptor.read_text()).get("weights", {}).get("nnue")
            if binding:
                models.append(descriptor.parent / binding["file"])
        games.extend(g["game_path"] for g in state["slots"] if g["status"] == "done" and g.get("game_path"))
    archive.archive(games, "games")
    archive.archive(metadata, "other")
    generations = sorted(snapshots.glob("*.json"), key=lambda p: p.stat().st_mtime, reverse=True)
    keep = set(metadata) | set(generations[: archive.config.get("local_runtime_snapshots", 8)])
    for old in generations:
        if old not in keep:
            with archive.locked() as db:
                row = db.execute("SELECT sha256 FROM files WHERE path=?", (str(old),)).fetchone()
                if row and archive_storage.digest(old) == row["sha256"]:
                    old.unlink()
                    db.execute("UPDATE files SET evicted=1 WHERE path=?", (str(old),))
    for name in config.get("model_roots", []):
        models.extend(p for p in Path(name).rglob("*") if p.suffix in (".nnue", ".bin", ".json") and p.is_file())
    archive.archive(models, "models")
    features = []
    for name in config.get("datasets", []):
        identity = json.loads((Path(name) / "dataset.json").read_text())
        features.extend(s["path"] for s in identity.get("feature_shards", []))
        archive.archive([Path(name) / "dataset.json", Path(name) / "schema.json"], "other")
    archive.archive(features, "features")
    archive.evict(cache_bytes=archive.config.get("feature_cache_bytes", 8 * archive_storage.GIB))
    for name in config.get("closed_training", []):
        run = Path(name)
        if json.loads((run / "status.json").read_text()).get("state") != "completed":
            raise ValueError("Refusing to archive an unfinished training run")
        import fcntl

        with (run / "supervisor.lock").open("a+") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            paths = list(run.glob("**/training.pt")) + list(run.glob("**/w*.training.pt"))
            archive.archive(paths, "closed-training")
            archive.evict(paths)
    # Exercise one actual restore into an isolated scratch location on every
    # maintenance pass, leaving source residency untouched.
    with archive.locked() as db:
        sample = db.execute("SELECT * FROM files WHERE category='games' ORDER BY last_used DESC LIMIT 1").fetchone()
    if sample:
        archive.verify_restore(sample["path"])
    status = archive.status()
    status["updated"] = time.time()
    status["filesystems"] = {
        rule["path"]: dict(
            free_bytes=shutil.disk_usage(rule["path"]).free,
            min_free_bytes=rule["min_free_bytes"],
            soft_free_bytes=rule.get("soft_free_bytes", rule["min_free_bytes"]),
        )
        for rule in archive.config.get("filesystems", [])
    }
    prior = archive.root / "maintenance-status.json"
    previous = json.loads(prior.read_text()) if prior.exists() else {}
    before = {row["category"]: row["bytes"] for row in previous.get("categories", [])}
    status["growth_since_previous_bytes"] = {
        row["category"]: row["bytes"] - before.get(row["category"], 0) for row in status["categories"]
    }
    archive_storage.atomic(prior, status)
    return status


if __name__ == "__main__":
    storage_guard.require_healthy(Path(os.environ.get("TAIKYOKU_STORAGE_CONFIG", "/etc/taikyoku/storage.json")))
    archive = archive_storage.configured()
    if archive is None:
        sys.exit("No archive policy installed; maintenance did not start")
    print(json.dumps(maintain(archive), indent=2))
