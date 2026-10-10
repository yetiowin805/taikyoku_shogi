"""Verified shared model snapshots and compressed teacher rollback backups."""
import contextlib
import fcntl
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sqlite3
import tempfile
import time


def digest(path, opener=open):
    value = hashlib.sha256()
    with opener(path, 'rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            value.update(chunk)
    return value.hexdigest()


def sync_directory(path):
    fd = os.open(path, os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def settings():
    path = Path(os.environ.get('TAIKYOKU_ARTIFACT_CONFIG', '/etc/taikyoku/artifact-storage.json'))
    if not path.exists():
        return {}
    config = json.loads(path.read_text())
    for mount in config.get('required_mounts', []):
        if not os.path.ismount(mount):
            raise ValueError(f'artifact volume is not mounted: {mount}')
    return config


def copy_verified(source, target, sha):
    fd, name = tempfile.mkstemp(prefix='.' + target.name + '-', dir=target.parent)
    temporary = Path(name)
    try:
        with os.fdopen(fd, 'wb') as out, Path(source).open('rb') as src:
            shutil.copyfileobj(src, out, 1024 * 1024)
            out.flush()
            os.fsync(out.fileno())
        if digest(temporary) != sha:
            raise ValueError(f'model changed while copying: {source}')
        temporary.chmod(0o444)
        temporary.replace(target)
        sync_directory(target.parent)
    finally:
        temporary.unlink(missing_ok=True)


def snapshot_blob(source, target, sha):
    """Share snapshot copies only; never alias a mutable training source inode."""
    if not re.fullmatch('[0-9a-f]{64}', sha):
        raise ValueError('invalid model hash')
    source, target = Path(source), Path(target)
    config = settings()
    if target.exists() and digest(target) != sha:
        raise ValueError(f'corrupt existing NNUE snapshot: {target}')
    if not config.get('model_store'):
        if not target.exists():
            copy_verified(source, target, sha)
        return
    store = Path(config['model_store'])
    if not store.is_absolute():
        raise ValueError('model_store must be absolute')
    store.mkdir(parents=True, exist_ok=True)
    canonical = store / (sha + '.nnue')
    with (store / '.lock').open('a+') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if canonical.exists():
            if digest(canonical) != sha:
                raise ValueError(f'corrupt shared model: {canonical}')
        elif target.is_file() and not target.is_symlink() and target.stat().st_dev == store.stat().st_dev:
            # Existing snapshots are independent immutable copies, not originals.
            os.link(target, canonical)
            canonical.chmod(0o444)
            sync_directory(store)
        else:
            copy_verified(source, canonical, sha)
        if target.is_symlink() and target.resolve() == canonical:
            return
        # Publish the reference without any gap in the old snapshot path.
        temporary = target.with_name(f'.{target.name}.{os.getpid()}.{time.time_ns()}.link')
        try:
            temporary.symlink_to(canonical)
            temporary.replace(target)
            sync_directory(target.parent)
        finally:
            temporary.unlink(missing_ok=True)


def compress_database(path):
    """Losslessly compress a closed rollback database; refuse live/hot journals."""
    path = Path(path)
    for suffix in ('-wal', '-journal'):
        journal = Path(str(path) + suffix)
        if journal.exists() and journal.stat().st_size:
            raise ValueError(f'backup has a journal; retain for inspection: {path}')
    before = path.stat()
    with contextlib.closing(sqlite3.connect(path.as_uri() + '?mode=ro', uri=True)) as db:
        if db.execute('PRAGMA quick_check').fetchall() != [('ok',)]:
            raise ValueError(f'invalid backup database: {path}')
    sha = digest(path)
    target = path.with_name(path.name + '.gz')
    fd, name = tempfile.mkstemp(prefix='.compress-', dir=path.parent)
    temporary = Path(name)
    try:
        with os.fdopen(fd, 'wb') as raw:
            with gzip.GzipFile(fileobj=raw, mode='wb', compresslevel=1, mtime=0) as out, path.open('rb') as src:
                shutil.copyfileobj(src, out, 1024 * 1024)
            raw.flush()
            os.fsync(raw.fileno())
        if digest(temporary, gzip.open) != sha:
            raise ValueError('backup compression verification failed')
        after = path.stat()
        if (before.st_ino, before.st_size, before.st_mtime_ns) != (after.st_ino, after.st_size, after.st_mtime_ns):
            raise ValueError('backup changed during compression')
        if target.exists():
            if digest(target, gzip.open) != sha:
                raise ValueError('existing compressed backup differs')
        else:
            temporary.replace(target)
            sync_directory(path.parent)
        path.unlink()
        sync_directory(path.parent)
        return dict(sha256=sha, original_bytes=before.st_size, compressed_bytes=target.stat().st_size)
    finally:
        temporary.unlink(missing_ok=True)


def teacher_backup(db, config_path):
    """Publish a complete compressed backup before changing teacher configuration."""
    config_path = Path(config_path)
    root = config_path.parent / 'teacher-backups'
    root.mkdir(exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix='.incomplete-', dir=root))
    # Leave incomplete snapshots for diagnosis; retention never deletes them.
    with (stage / config_path.name).open('wb') as out:
        out.write(config_path.read_bytes())
        out.flush()
        os.fsync(out.fileno())
    database = stage / 'training-labels.sqlite'
    with contextlib.closing(sqlite3.connect(database)) as dest:
        db.backup(dest)
    report = compress_database(database)
    with (stage / 'backup.json').open('w') as out:
        json.dump(dict(format='compressed-teacher-backup-v1', **report), out)
        out.flush()
        os.fsync(out.fileno())
    sync_directory(stage)
    target = root / str(time.time_ns())
    stage.rename(target)
    sync_directory(root)
    return target


def prune_teacher_backups(root, keep=3):
    """Only retire completed backups created by this version after a successful switch."""
    if type(keep) is not int or keep < 2:
        raise ValueError('keep at least two teacher rollback backups')
    candidates = []
    for path in Path(root).iterdir():
        marker = path / 'backup.json'
        if path.is_symlink() or not path.name.isdigit() or not marker.is_file():
            continue
        if (json.loads(marker.read_text()).get('format') == 'compressed-teacher-backup-v1'
                and (path / 'training-labels.sqlite.gz').is_file()
                and (path / 'dual-label-config.json').is_file()):
            candidates.append(path)
    for path in sorted(candidates, key=lambda p: int(p.name))[:-keep]:
        shutil.rmtree(path)
    sync_directory(root)
