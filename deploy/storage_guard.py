#!/usr/bin/env python3
"""Check storage before startup; --enforce stops services before disk exhaustion."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time


def problems(config):
    errors = []
    for mount in config.get('required_mounts', []):
        if not os.path.ismount(mount):
            errors.append(f'required data volume is not mounted: {mount}')
    for item in config['filesystems']:
        try:
            free = shutil.disk_usage(item['path']).free
            if free < item['min_free_bytes']:
                errors.append(f"{item['path']}: only {free / 2**30:.2f} GiB free; "
                              f"requires {item['min_free_bytes'] / 2**30:.2f} GiB")
        except OSError as exc:
            errors.append(f"cannot check {item['path']}: {exc}")
    if config.get('state'):
        try:
            state = json.loads(Path(config['state']).read_text())
            if not isinstance(state.get('slots'), list):
                raise ValueError('missing slot ledger')
        except (OSError, ValueError) as exc:
            errors.append(f"invalid tournament checkpoint: {exc}")
    return errors


def require_healthy(path):
    """An installed policy also applies to ad-hoc and admission supervisors."""
    path = Path(path)
    if path.exists():
        errors = problems(json.loads(path.read_text()))
        if errors:
            raise RuntimeError('STORAGE GUARD: ' + '; '.join(errors))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('config', type=Path)
    p.add_argument('--enforce', action='store_true')
    args = p.parse_args()
    config = json.loads(args.config.read_text())
    errors = problems(config)
    # /run is tmpfs, so the failure remains visible even if a data disk is full.
    status = Path(config.get('status', '/run/taikyoku-storage-status.json'))
    temporary = status.with_name(f'.{status.name}.{os.getpid()}.tmp')
    temporary.write_text(json.dumps(dict(ok=not errors, errors=errors, updated=time.time()))+'\n')
    temporary.replace(status)
    if errors:
        print('STORAGE GUARD: ' + '; '.join(errors), file=sys.stderr, flush=True)
        if args.enforce:
            for unit in config.get('stop_units', []):
                subprocess.run(['systemctl', 'stop', unit], check=True, timeout=180)
            print('Tournament and analysis stopped; resolve storage and restart explicitly.', file=sys.stderr)
        else:
            print('Tournament/analysis did not start.', file=sys.stderr)
        raise SystemExit(1)


if __name__ == '__main__':
    main()
