"""Store exact search payloads once; keep moment metadata queryable as JSON."""

import argparse
import contextlib
import hashlib
import json
import sqlite3
from pathlib import Path


def decode_search(db, payload):
    search = json.loads(payload)
    reference = search.pop("source_move_ref", None)
    if reference is not None:
        row = db.execute("SELECT payload FROM source_moves WHERE game_hash=? AND ply=?", reference).fetchone()
        if row is None:
            raise ValueError("Missing source move")
        search["original_move"] = json.loads(row[0])
    return search


def encode_search(db, search):
    result = dict(search)
    if "original_move" in result and result.get("game_hash") and type(result.get("ply")) is int:
        db.execute(
            "CREATE TABLE IF NOT EXISTS source_moves(game_hash TEXT,ply INTEGER,payload TEXT,PRIMARY KEY(game_hash,ply))"
        )
        reference = [result["game_hash"], result["ply"]]
        original = result.pop("original_move")
        row = db.execute("SELECT payload FROM source_moves WHERE game_hash=? AND ply=?", reference).fetchone()
        if row and json.loads(row[0]) != original:
            raise ValueError("Source move changed under the same game identity")
        if not row:
            db.execute("INSERT INTO source_moves VALUES(?,?,?)", (*reference, json.dumps(original)))
        result["source_move_ref"] = reference
    return json.dumps(result, separators=(",", ":"), sort_keys=True)


def encode(db, moment):
    result = dict(moment)
    for field in ("searches", "previous_teacher_searches"):
        if field not in result:
            continue
        refs = []
        for search in result[field]:
            if set(search) == {"search_ref"}:
                refs.append(search)
                continue
            payload = encode_search(db, search)
            key = search.get("key") or hashlib.sha256(payload.encode()).hexdigest()
            saved = db.execute("SELECT payload FROM searches WHERE key=?", (key,)).fetchone()
            if saved and decode_search(db, saved[0]) != search:
                # Legacy moments sometimes retained a slim or annotated view of
                # a request. Preserve those exact bytes under a content key too.
                key = "view-" + hashlib.sha256(payload.encode()).hexdigest()
                saved = db.execute("SELECT payload FROM searches WHERE key=?", (key,)).fetchone()
            if not saved:
                db.execute("INSERT INTO searches VALUES(?,?)", (key, payload))
            refs.append(dict(search_ref=key))
        result[field] = refs
    return json.dumps(result, separators=(",", ":"))


def decode(db, payload):
    result = json.loads(payload) if isinstance(payload, str) else dict(payload)
    for field in ("searches", "previous_teacher_searches"):
        if field not in result:
            continue
        searches = []
        for search in result[field]:
            if set(search) == {"search_ref"}:
                row = db.execute("SELECT payload FROM searches WHERE key=?", (search["search_ref"],)).fetchone()
                if row is None:
                    raise ValueError("Missing referenced search: " + search["search_ref"])
                search = decode_search(db, row[0])
            searches.append(search)
        result[field] = searches
    return result


def batches(db, table, key):
    last = None
    while True:
        where = "" if last is None else f"WHERE {key}>?"
        rows = db.execute(
            f"SELECT {key},payload FROM {table} {where} ORDER BY {key} LIMIT 128", () if last is None else (last,)
        ).fetchall()
        if not rows:
            return
        yield from rows
        last = rows[-1][0]


def migrate(path):
    """Caller must hold dual-label.lock with collectors stopped; retain rollback."""
    path = Path(path)
    backup = path.with_name(path.name + ".before-normalization")
    with contextlib.closing(sqlite3.connect(path)) as db:
        db.execute("PRAGMA busy_timeout=30000")
        db.execute("CREATE TABLE IF NOT EXISTS storage_migrations(name TEXT PRIMARY KEY)")
        db.commit()
        if db.execute("SELECT 1 FROM storage_migrations WHERE name='search-refs-v1'").fetchone():
            return
        if backup.exists():
            raise ValueError("Existing pre-migration backup needs inspection")
        with contextlib.closing(sqlite3.connect(backup)) as saved:
            db.backup(saved)
            if saved.execute("PRAGMA integrity_check").fetchall() != [("ok",)]:
                raise ValueError("Invalid label rollback backup")
        try:
            db.execute("BEGIN IMMEDIATE")
            for key, payload in batches(db, "searches", "key"):
                db.execute(
                    "UPDATE searches SET payload=? WHERE key=?", (encode_search(db, decode_search(db, payload)), key)
                )
            for key, payload in batches(db, "moments", "id"):
                original = decode(db, payload)
                packed = encode(db, original)
                if decode(db, packed) != original:
                    raise ValueError("Moment migration changed labels")
                db.execute("UPDATE moments SET payload=? WHERE id=?", (packed, key))
            db.execute("INSERT INTO storage_migrations VALUES('search-refs-v1')")
            db.commit()
        except BaseException:
            db.rollback()
            raise
        db.execute("PRAGMA wal_checkpoint(TRUNCATE)")
        db.execute("VACUUM")
        if db.execute("PRAGMA integrity_check").fetchall() != [("ok",)]:
            raise ValueError("Migrated label catalogue failed integrity check; rollback retained")


if __name__ == "__main__":
    import fcntl

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("catalogue", type=Path)
    args = parser.parse_args()
    with (args.catalogue.parent / "dual-label.lock").open("a+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        migrate(args.catalogue)
