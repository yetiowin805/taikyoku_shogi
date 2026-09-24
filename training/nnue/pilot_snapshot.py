"""Read-only VPS snapshot: immutable completed games and committed training labels.

Run from the repository root; stdout is a tar.gz stream. No live files are changed.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import sqlite3
import sys
import tarfile
import time


def main():
    p = argparse.ArgumentParser()
    p.add_argument('runs', nargs='+')
    a = p.parse_args()
    manifest = dict(version=1, timestamp=time.time(), games=[], labels=[])
    with tarfile.open(fileobj=sys.stdout.buffer, mode='w|gz', compresslevel=1) as tar:
        def put(name, data):
            entry = tarfile.TarInfo(name)
            entry.size = len(data)
            tar.addfile(entry, io.BytesIO(data))

        for name in a.runs:
            run = Path('data/raw/tourney') / name
            state = json.loads((run / 'state.json').read_text())
            put(f'states/{name}.json', json.dumps(state).encode())
            for slot in state['slots']:
                if slot['status'] != 'done' or not slot.get('game_path'):
                    continue
                path = Path(slot['game_path'])
                data = path.read_bytes()
                g = json.loads(data)
                if g.get('abort_reason') or not g.get('result'):
                    continue
                dest = f'games/{name}/{path.name}'
                put(dest, data)
                manifest['games'].append(dict(path=dest, sha256=hashlib.sha256(data).hexdigest(),
                                              run=name, pair_seed=slot['start_seed']))
            db = run / 'analysis/training-labels.sqlite'
            if db.exists():
                with sqlite3.connect(f'file:{db}?mode=ro', uri=True) as conn:
                    labels = [json.loads(x[0]) for x in conn.execute(
                        "SELECT payload FROM moments WHERE status='completed' ORDER BY id")]
                dest = f'labels/{name}.jsonl'
                put(dest, ''.join(json.dumps(x) + '\n' for x in labels).encode())
                manifest['labels'].append(dest)
        put('snapshot.json', json.dumps(manifest, indent=2).encode())


if __name__ == '__main__':
    main()
