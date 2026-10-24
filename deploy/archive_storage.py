"""Verified immutable archive packs, process-held pins, and peak-space admission.

Policy and inventories are host-local. No remote deletion is performed. Local
removal requires a full remote readback and an exclusive dependency pin.
"""

import argparse
import contextlib
import fcntl
import hashlib
import json
import os
import shutil
import sqlite3
import subprocess
import tarfile
import tempfile
import time
import uuid
from pathlib import Path

GIB = 1024**3
CHUNK = 64 * 1024**2


def digest(path):
    value = hashlib.sha256()
    with Path(path).open("rb") as source:
        for data in iter(lambda: source.read(1024**2), b""):
            value.update(data)
    return value.hexdigest()


def atomic(path, value):
    path = Path(path)
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as out:
        temporary = Path(out.name)
        try:
            out.write((json.dumps(value, indent=2) + "\n").encode())
            out.flush()
            os.fsync(out.fileno())
            temporary.replace(path)
            sync_dir(path.parent)
        finally:
            temporary.unlink(missing_ok=True)


def sync_dir(path):
    fd = os.open(path, os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def process_identity(pid):
    try:
        return Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[19]
    except (OSError, IndexError):
        return None


def configured():
    path = Path(os.environ.get("TAIKYOKU_ARCHIVE_CONFIG", "/etc/taikyoku/archive-storage.json"))
    return Archive(json.loads(path.read_text())) if path.exists() else None


class Lease:
    def __init__(self, archive, token, locks):
        self.archive, self.token, self.locks = archive, token, locks

    def close(self):
        if self.token:
            with self.archive.locked() as db:
                db.execute("DELETE FROM reservations WHERE token=?", (self.token,))
            self.token = None
        for lock in self.locks:
            lock.close()
        self.locks.clear()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def __del__(self):
        # flock pins also disappear after SIGKILL. Reservations are reaped using
        # PID + start time, never an expiry that could evict a slow live trainer.
        with contextlib.suppress(Exception):
            self.close()


class Archive:
    def __init__(self, config):
        self.config = config
        self.root = Path(config["root"])
        if not self.root.is_absolute():
            raise ValueError("Archive root must be absolute")
        for mount in config.get("required_mounts", []):
            if not os.path.ismount(mount):
                raise ValueError("Archive volume is not mounted: " + mount)
        self.root.mkdir(parents=True, exist_ok=True)
        (self.root / "locks").mkdir(exist_ok=True)
        self.remote = config["remote"].rstrip("/")
        self.client = config.get("rclone", "rclone")
        with self.locked() as db:
            db.executescript("""
                CREATE TABLE IF NOT EXISTS files(
                    path TEXT PRIMARY KEY, sha256 TEXT, bytes INTEGER, mode INTEGER,
                    pack TEXT, member TEXT, last_used REAL, evicted INTEGER, category TEXT);
                CREATE TABLE IF NOT EXISTS packs(hash TEXT PRIMARY KEY, bytes INTEGER, raw_bytes INTEGER, remote TEXT, encoding TEXT);
                CREATE TABLE IF NOT EXISTS reservations(
                    token TEXT, pid INTEGER, identity TEXT, device INTEGER, bytes INTEGER);
            """)
            legacy = config.get("legacy_manifest")
            if legacy:
                manifest = json.loads(Path(legacy).read_text())
                if manifest.get("state") != "completed":
                    raise ValueError("Legacy offload did not complete")
                for entry in manifest["files"]:
                    if not entry.get("full_download_and_restore_verified_at") or not entry.get("local_evicted"):
                        raise ValueError("Legacy file lacks verified readback")
                    db.execute(
                        "INSERT OR IGNORE INTO packs VALUES(?,?,?,?,?)",
                        (
                            entry["archive_sha256"],
                            entry["archive_bytes"],
                            entry["bytes"],
                            entry["remote"],
                            "zstd-file" if entry["encoding"] == "zstd" else "identity",
                        ),
                    )
                    db.execute(
                        "INSERT OR IGNORE INTO files VALUES(?,?,?,?,?,?,?,?,?)",
                        (
                            entry["path"],
                            entry["sha256"],
                            entry["bytes"],
                            entry["mode"],
                            entry["archive_sha256"],
                            entry["sha256"],
                            time.time(),
                            int(not Path(entry["path"]).exists()),
                            "closed-training",
                        ),
                    )

    @contextlib.contextmanager
    def locked(self):
        with (self.root / "index.lock").open("a+") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            with contextlib.closing(sqlite3.connect(self.root / "index.sqlite", timeout=60)) as db:
                db.row_factory = sqlite3.Row
                with db:
                    yield db

    @contextlib.contextmanager
    def staging(self, prefix, parent):
        work = Path(tempfile.mkdtemp(prefix=prefix, dir=parent))
        try:
            yield work
        except BaseException as error:
            with contextlib.suppress(OSError):
                atomic(work / "failure.json", dict(error=str(error), updated=time.time()))
            raise
        else:
            shutil.rmtree(work)

    def file_lock(self, path, exclusive=False, nonblocking=False):
        key = hashlib.sha256(str(Path(path).resolve()).encode()).hexdigest()
        lock = (self.root / "locks" / key).open("a+")
        try:
            fcntl.flock(lock, (fcntl.LOCK_EX if exclusive else fcntl.LOCK_SH) | (fcntl.LOCK_NB if nonblocking else 0))
            return lock
        except BaseException:
            lock.close()
            raise

    def copy(self, source, destination, immutable=False):
        if self.config.get("local_remote"):
            target = Path(destination)
            target.parent.mkdir(parents=True, exist_ok=True)
            if immutable and target.exists():
                if digest(target) != digest(source):
                    raise ValueError("Immutable object changed")
                return
            shutil.copyfile(source, target)
        else:
            command = [
                self.client,
                "copyto",
                str(source),
                str(destination),
                "--timeout",
                "60s",
                "--contimeout",
                "20s",
                "--retries",
                "2",
                "--multi-thread-streams",
                "0",
            ]
            if immutable:
                command.append("--immutable")
            subprocess.run(command, check=True)

    def download(self, source, destination, size):
        if self.config.get("local_remote"):
            shutil.copyfile(source, destination)
        else:
            with Path(destination).open("wb") as out:
                for offset in range(0, size, CHUNK):
                    count = min(CHUNK, size - offset)
                    subprocess.run(
                        [
                            self.client,
                            "cat",
                            source,
                            "--offset",
                            str(offset),
                            "--count",
                            str(count),
                            "--timeout",
                            "60s",
                            "--contimeout",
                            "20s",
                            "--low-level-retries",
                            "2",
                        ],
                        stdout=out,
                        check=True,
                    )
                    if out.tell() != offset + count:
                        raise ValueError("Short archive range read")
                out.flush()
                os.fsync(out.fileno())
        if Path(destination).stat().st_size != size:
            raise ValueError("Wrong archive byte count")

    def reserve_bytes(self, path, needed, db, soft=False):
        path = Path(path)
        while not path.exists():
            path = path.parent
        device = path.stat().st_dev
        rules = self.config.get("filesystems", [])
        minimum = max(
            (
                r.get("soft_free_bytes" if soft else "min_free_bytes", r["min_free_bytes"])
                for r in rules
                if Path(r["path"]).stat().st_dev == device
            ),
            default=self.config.get("min_free_bytes", 5 * GIB),
        )
        for row in db.execute("SELECT DISTINCT token,pid,identity FROM reservations").fetchall():
            if process_identity(row["pid"]) != row["identity"]:
                db.execute("DELETE FROM reservations WHERE token=?", (row["token"],))
        held = db.execute("SELECT coalesce(sum(bytes),0) FROM reservations WHERE device=?", (device,)).fetchone()[0]
        if shutil.disk_usage(path).free < minimum + held + needed:
            raise ValueError(f"Insufficient peak storage on {path}: need {needed} bytes plus reserve and other jobs")
        return device

    def pin(self, paths, requirements=None, mutable_paths=()):
        """Pin all inputs and reserve the complete missing set before any download."""
        paths = sorted({str(Path(p).resolve()) for p in paths})
        mutable_paths = {str(Path(p).resolve()) for p in mutable_paths}
        locks = []
        token = uuid.uuid4().hex
        try:
            with self.locked() as db:
                needs = dict(requirements or {})
                staging = {}
                for path in paths:
                    locks.append(self.file_lock(path, nonblocking=True))
                    if not Path(path).exists():
                        row = db.execute("SELECT * FROM files WHERE path=?", (path,)).fetchone()
                        if row is None:
                            raise FileNotFoundError("No verified archive for input: " + path)
                        pack = db.execute("SELECT bytes,raw_bytes FROM packs WHERE hash=?", (row["pack"],)).fetchone()
                        parent = Path(path).parent
                        needs[parent] = needs.get(parent, 0) + row["bytes"]
                        existing = parent
                        while not existing.exists():
                            existing = existing.parent
                        device = existing.stat().st_dev
                        old = staging.get(device, (parent, 0))
                        staging[device] = (parent, max(old[1], pack["bytes"] + pack["raw_bytes"]))
                for parent, size in staging.values():
                    needs[parent] = needs.get(parent, 0) + size
                devices = {}
                for path, size in needs.items():
                    parent = Path(path)
                    while not parent.exists():
                        parent = parent.parent
                    dev = parent.stat().st_dev
                    old = devices.get(dev, (parent, 0))
                    devices[dev] = (parent, old[1] + size)
                for parent, size in devices.values():
                    device = self.reserve_bytes(parent, size, db, soft=True)
                    db.execute(
                        "INSERT INTO reservations VALUES(?,?,?,?,?)",
                        (token, os.getpid(), process_identity(os.getpid()), device, size),
                    )
            lease = Lease(self, token, locks)
            for path in paths:
                self.hydrate(path, pinned=True, allow_changed=path in mutable_paths)
            return lease
        except BaseException:
            for lock in locks:
                lock.close()
            with self.locked() as db:
                db.execute("DELETE FROM reservations WHERE token=?", (token,))
            raise

    def hydrate(self, path, pinned=False, allow_changed=False):
        path = Path(path).resolve()
        if not pinned:
            with self.pin([path]):
                return
        with self.locked() as db:
            row = db.execute("SELECT * FROM files WHERE path=?", (str(path),)).fetchone()
            if path.exists():
                if row and not allow_changed and (path.stat().st_size != row["bytes"] or digest(path) != row["sha256"]):
                    raise ValueError("Refusing different existing content: " + str(path))
                if row:
                    db.execute("UPDATE files SET last_used=?,evicted=0 WHERE path=?", (time.time(), str(path)))
                return
            if row is None:
                raise FileNotFoundError(path)
            pack = db.execute("SELECT * FROM packs WHERE hash=?", (row["pack"],)).fetchone()
            path.parent.mkdir(parents=True, exist_ok=True)
            self.restore_content(row, pack, path)
            db.execute("UPDATE files SET last_used=?,evicted=0 WHERE path=?", (time.time(), str(path)))

    def restore_content(self, row, pack, path):
        path = Path(path)
        path.parent.mkdir(parents=True, exist_ok=True)
        with self.staging(".hydrate-", path.parent) as temporary:
            work = Path(temporary)
            compressed, raw, restored = work / "pack.zst", work / "pack.tar", work / "restored"
            self.download(pack["remote"], compressed, pack["bytes"])
            if digest(compressed) != row["pack"]:
                raise ValueError("Archive hash mismatch")
            if pack["encoding"] == "identity":
                shutil.copyfile(compressed, restored)
            else:
                with raw.open("wb") as out:
                    subprocess.run(["zstd", "-q", "-d", "-c", str(compressed)], stdout=out, check=True)
                if pack["encoding"] == "zstd-file":
                    raw.replace(restored)
                elif pack["encoding"] == "tar-zstd":
                    with (
                        tarfile.open(raw) as bundle,
                        bundle.extractfile(row["member"]) as source,
                        restored.open("wb") as out,
                    ):
                        shutil.copyfileobj(source, out)
                        out.flush()
                        os.fsync(out.fileno())
                else:
                    raise ValueError("Unknown archive encoding")
            if restored.stat().st_size != row["bytes"] or digest(restored) != row["sha256"]:
                raise ValueError("Restored content mismatch")
            restored.chmod(row["mode"])
            # Publication never overwrites a different file, even if another
            # consumer raced the download. link is atomic and no-clobber.
            try:
                os.link(restored, path)
            except FileExistsError:
                if digest(path) != row["sha256"]:
                    raise ValueError("Refusing different restore target") from None
            sync_dir(path.parent)

    def verify_restore(self, source):
        with self.locked() as db:
            row = db.execute("SELECT * FROM files WHERE path=?", (str(Path(source).resolve()),)).fetchone()
            pack = db.execute("SELECT * FROM packs WHERE hash=?", (row["pack"],)).fetchone()
            self.reserve_bytes(self.root, row["bytes"] + pack["bytes"] + pack["raw_bytes"], db)
            with tempfile.TemporaryDirectory(prefix=".restore-drill-", dir=self.root) as directory:
                self.restore_content(row, pack, Path(directory) / "restored")
        return True

    def publish_index(self, db):
        data = dict(
            version=1,
            updated=time.time(),
            files=[dict(r) for r in db.execute("SELECT * FROM files ORDER BY path")],
            packs=[dict(r) for r in db.execute("SELECT * FROM packs ORDER BY hash")],
        )
        index = self.root / "index.json"
        atomic(index, data)
        # Keep immutable catalogue generations too; no remote retention until
        # reference-aware expiry has been explicitly configured.
        self.copy(index, self.remote + "/indexes/" + digest(index) + ".json", immutable=True)
        self.copy(index, self.remote + "/index.json")
        readback = self.root / ".index-readback.json"
        self.download(self.remote + "/index.json", readback, index.stat().st_size)
        if digest(readback) != digest(index):
            raise ValueError("Remote recovery index verification failed")
        readback.unlink()

    def archive(self, paths, category="other"):
        """Archive immutable/closed inputs only. This method never evicts them."""
        pending = []
        with self.locked() as db:
            for path in sorted({str(Path(p).resolve()) for p in paths}):
                p = Path(path)
                if not p.is_file():
                    # Previously evicted files already have a durable index.
                    if db.execute("SELECT 1 FROM files WHERE path=?", (path,)).fetchone():
                        continue
                    raise FileNotFoundError(path)
                h = digest(p)
                previous = db.execute("SELECT * FROM files WHERE path=?", (path,)).fetchone()
                if previous:
                    if previous["sha256"] == h:
                        continue
                    if category != "closed-training":
                        raise ValueError("Archived immutable source changed: " + path)
                    # A completed continuation can publish a new generation at
                    # its stable resume path. Preserve the old remote object and
                    # index generation; replace residency only after readback.
                pending.append((p, h, p.stat()))
            batch = []
            total = 0
            for item in pending:
                if batch and total + item[2].st_size > self.config.get("pack_bytes", 128 * 1024**2):
                    self._pack(db, batch, category)
                    batch, total = [], 0
                batch.append(item)
                total += item[2].st_size
            if batch:
                self._pack(db, batch, category)
            self.publish_index(db)
        return len(pending)

    def _pack(self, db, batch, category):
        self.reserve_bytes(self.root, 3 * (sum(s.st_size for _, _, s in batch) + 10240 + len(batch) * 4096), db)
        with self.staging(".pack-", self.root) as directory:
            work = Path(directory)
            raw, compressed, downloaded = work / "pack.tar", work / "pack.zst", work / "readback.zst"
            with tarfile.open(raw, "w") as pack:
                for path, h, _ in batch:
                    pack.add(path, arcname=h, recursive=False)
            subprocess.run(["zstd", "-q", "-T1", "-3", str(raw), "-o", str(compressed)], check=True)
            pack_hash, pack_size = digest(compressed), compressed.stat().st_size
            remote = self.remote + "/objects/" + pack_hash + ".tar.zst"
            self.copy(compressed, remote, immutable=True)
            self.download(remote, downloaded, pack_size)
            if digest(downloaded) != pack_hash:
                raise ValueError("Remote pack hash mismatch; sources retained")
            with raw.open("wb") as out:
                subprocess.run(["zstd", "-q", "-d", "-c", str(downloaded)], stdout=out, check=True)
            with tarfile.open(raw) as pack:
                for path, h, before in batch:
                    with pack.extractfile(h) as source:
                        value = hashlib.sha256()
                        size = 0
                        for data in iter(lambda: source.read(1024**2), b""):
                            value.update(data)
                            size += len(data)
                    after = path.stat()
                    if (
                        value.hexdigest() != h
                        or size != before.st_size
                        or digest(path) != h
                        or (after.st_ino, after.st_size, after.st_mtime_ns)
                        != (before.st_ino, before.st_size, before.st_mtime_ns)
                    ):
                        raise ValueError("Source changed during archive; sources retained")
            db.execute(
                "INSERT OR IGNORE INTO packs VALUES(?,?,?,?,?)",
                (pack_hash, pack_size, raw.stat().st_size, remote, "tar-zstd"),
            )
            for path, h, before in batch:
                db.execute(
                    "INSERT OR REPLACE INTO files VALUES(?,?,?,?,?,?,?,?,?)",
                    (str(path), h, before.st_size, before.st_mode & 0o777, pack_hash, h, time.time(), 0, category),
                )
            self.publish_index(db)
            db.commit()  # Each verified pack survives an interrupted later pack.
            print(
                json.dumps(
                    dict(
                        archived_files=len(batch),
                        source_bytes=sum(x[2].st_size for x in batch),
                        archive_bytes=pack_size,
                    )
                ),
                flush=True,
            )

    def evict(self, paths=None, cache_bytes=None):
        removed = 0
        with self.locked() as db:
            rows = db.execute(
                "SELECT * FROM files WHERE evicted=0 AND category IN ('features','closed-training') ORDER BY last_used,path"
            ).fetchall()
            total = sum(r["bytes"] for r in rows if Path(r["path"]).exists())
            wanted = None if paths is None else {str(Path(p).resolve()) for p in paths}
            for row in rows:
                if wanted is not None and row["path"] not in wanted:
                    continue
                if cache_bytes is not None and total <= cache_bytes:
                    break
                try:
                    lock = self.file_lock(row["path"], exclusive=True, nonblocking=True)
                except BlockingIOError:
                    continue
                with lock:
                    p = Path(row["path"])
                    if p.exists():
                        if p.stat().st_size != row["bytes"] or digest(p) != row["sha256"]:
                            raise ValueError("Refusing changed eviction candidate: " + str(p))
                        atomic(Path(str(p) + ".cold.json"), dict(row))
                        p.unlink()
                        sync_dir(p.parent)
                    db.execute("UPDATE files SET evicted=1 WHERE path=?", (row["path"],))
                    total -= row["bytes"]
                    removed += row["bytes"]
            self.publish_index(db)
        return removed

    def recover_index(self):
        """Recover metadata on a new host. Restores retain their original paths."""
        with self.locked() as db:
            if db.execute("SELECT count(*) FROM files").fetchone()[0]:
                raise ValueError("Refusing to replace a populated archive index")
            target = self.root / "recovered-index.json"
            self.copy(self.remote + "/index.json", target)
            data = json.loads(target.read_text())
            if data["version"] != 1:
                raise ValueError("Unknown archive index version")
            for row in data["packs"]:
                db.execute(
                    "INSERT INTO packs VALUES(?,?,?,?,?)",
                    (row["hash"], row["bytes"], row["raw_bytes"], row["remote"], row["encoding"]),
                )
            for row in data["files"]:
                if not Path(row["path"]).is_absolute() or row["pack"] not in {p["hash"] for p in data["packs"]}:
                    raise ValueError("Invalid archive index")
                db.execute(
                    "INSERT INTO files VALUES(?,?,?,?,?,?,?,?,?)",
                    tuple(
                        row[k]
                        for k in (
                            "path",
                            "sha256",
                            "bytes",
                            "mode",
                            "pack",
                            "member",
                            "last_used",
                            "evicted",
                            "category",
                        )
                    ),
                )

    def status(self):
        with self.locked() as db:
            categories = [
                dict(r)
                for r in db.execute(
                    "SELECT category,count(*) AS files,sum(bytes) AS bytes,sum(CASE WHEN evicted=1 THEN bytes ELSE 0 END) AS cold_bytes FROM files GROUP BY category"
                )
            ]
            return dict(categories=categories, packs=db.execute("SELECT count(*),sum(bytes) FROM packs").fetchone()[:])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["archive", "restore", "evict", "status", "recover-index"])
    parser.add_argument("paths", nargs="*", type=Path)
    parser.add_argument(
        "--category", choices=["features", "closed-training", "games", "models", "labels", "other"], default="other"
    )
    parser.add_argument("--cache-bytes", type=int)
    args = parser.parse_args()
    archive = configured()
    if archive is None:
        raise ValueError("An explicit archive-storage policy is required")
    if args.action == "archive":
        archive.archive(args.paths, args.category)
    elif args.action == "restore":
        with archive.pin(args.paths):
            pass
    elif args.action == "evict":
        if not args.paths and args.cache_bytes is None:
            parser.error("evict requires paths or --cache-bytes")
        print(json.dumps(dict(removed_bytes=archive.evict(args.paths or None, args.cache_bytes))))
    elif args.action == "recover-index":
        archive.recover_index()
    else:
        print(json.dumps(archive.status(), indent=2))


if __name__ == "__main__":
    main()
