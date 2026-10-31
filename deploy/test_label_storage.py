import json
import sqlite3
import tempfile
import unittest
from pathlib import Path

import label_storage


class LabelStorageTests(unittest.TestCase):
    def test_migration_preserves_teacher_history_original_moves_and_rollback(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "labels.sqlite"
            search = dict(
                key="request",
                score=123,
                ply=4,
                game_hash="game",
                model_sha256="teacher",
                original_move=dict(eval=23, color="Black"),
                iterations=[dict(completed_depth=4, score=123)],
            )
            moment = dict(searches=[search], previous_teacher_searches=[search], winner_teacher={"name": "ab"})
            with sqlite3.connect(path) as db:
                db.executescript(
                    "CREATE TABLE searches(key TEXT PRIMARY KEY,payload TEXT); CREATE TABLE moments(id TEXT PRIMARY KEY,payload TEXT);"
                )
                db.execute("INSERT INTO searches VALUES(?,?)", ("request", json.dumps(search)))
                db.execute("INSERT INTO moments VALUES(?,?)", ("moment", json.dumps(moment)))
            label_storage.migrate(path)
            label_storage.migrate(path)
            with sqlite3.connect(path) as db:
                stored = db.execute("SELECT payload FROM moments").fetchone()[0]
                self.assertEqual(label_storage.decode(db, stored), moment)
                self.assertNotIn("original_move", stored)
                self.assertEqual(db.execute("SELECT count(*) FROM searches").fetchone()[0], 1)
                self.assertEqual(db.execute("SELECT count(*) FROM source_moves").fetchone()[0], 1)
                self.assertEqual(
                    db.execute("SELECT json_extract(payload,'$.winner_teacher.name') FROM moments").fetchone()[0], "ab"
                )
            with sqlite3.connect(str(path) + ".before-normalization") as db:
                self.assertEqual(json.loads(db.execute("SELECT payload FROM moments").fetchone()[0]), moment)

    def test_distinct_legacy_view_and_missing_reference_fail_loudly(self):
        with sqlite3.connect(":memory:") as db:
            db.execute("CREATE TABLE searches(key TEXT PRIMARY KEY,payload TEXT)")
            original = dict(key="same", score=1)
            db.execute("INSERT INTO searches VALUES(?,?)", ("same", json.dumps(original)))
            moment = dict(searches=[dict(key="same", score=2)])
            self.assertEqual(label_storage.decode(db, label_storage.encode(db, moment)), moment)
            with self.assertRaisesRegex(ValueError, "Missing referenced"):
                label_storage.decode(db, json.dumps(dict(searches=[dict(search_ref="missing")])))
