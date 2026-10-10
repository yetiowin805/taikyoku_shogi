#!/usr/bin/env python3
"""Compact immutable snapshots and closed legacy teacher backups; dry-run by default."""
import argparse
import fcntl
import json
from pathlib import Path
import re

import artifact_storage as storage


def compact(run, roots, apply=False):
    control = run / 'analysis'
    report = dict(models=[], backups=[], retained_for_inspection=[])
    with (control / 'dual-label.lock').open('a+') as lock:
        # The collector must be stopped; games may continue using their original models.
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        for root in roots:
            for path in sorted(root.rglob('*.nnue')):
                if (path.parent.name != 'models' or path.parent.parent.name not in ('analysis', 'admission-bindings')
                        or path.is_symlink() or not re.fullmatch('[0-9a-f]{64}', path.stem)):
                    continue
                size = path.stat().st_size
                if apply:
                    if not storage.settings().get('model_store'):
                        raise ValueError('configure a shared model_store before migration')
                    storage.snapshot_blob(path, path, path.stem)
                report['models'].append(dict(path=str(path), bytes=size))
        backup_root = control / 'teacher-backups'
        if backup_root.exists():
            for path in sorted(backup_root.iterdir()):
                database = path / 'training-labels.sqlite'
                if path.is_symlink() or not path.name.isdigit() or not database.is_file():
                    continue
                if any((Path(str(database)+suffix).exists() and Path(str(database)+suffix).stat().st_size)
                       for suffix in ('-journal', '-wal')):
                    report['retained_for_inspection'].append(str(path))
                    continue
                item = dict(path=str(path), original_bytes=database.stat().st_size)
                if apply:
                    item.update(storage.compress_database(database))
                    # Legacy archives are retained; automatic pruning owns only new-format backups.
                    (path / 'archive.json').write_text(json.dumps(item, indent=2)+'\n')
                report['backups'].append(item)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run-dir', type=Path, required=True)
    parser.add_argument('--snapshot-root', type=Path, action='append', default=[])
    parser.add_argument('--apply', action='store_true')
    args = parser.parse_args()
    report = compact(args.run_dir.resolve(), [p.resolve() for p in args.snapshot_root], args.apply)
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
