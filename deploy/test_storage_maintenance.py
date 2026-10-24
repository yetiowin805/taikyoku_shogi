import json
import sqlite3
import tempfile
import unittest
from pathlib import Path

import archive_storage
import storage_maintenance


class MaintenanceTests(unittest.TestCase):
    def test_backup_closure_restore_drill_and_feature_eviction_keep_games_hot(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            run = root / "run"
            (run / "analysis").mkdir(parents=True)
            game = run / "slot1.json"
            game.write_text(json.dumps(dict(result="Draw", moves=[])))
            weights = root / "weights.bin"
            weights.write_bytes(b"network")
            model = root / "model.json"
            model.write_text(json.dumps(dict(weights=dict(nnue=dict(file=weights.name)))))
            (run / "state.json").write_text(
                json.dumps(dict(entrants=[dict(model=str(model))], slots=[dict(status="done", game_path=str(game))]))
            )
            labels = run / "analysis/training-labels.sqlite"
            with sqlite3.connect(labels) as db:
                db.execute("CREATE TABLE evidence(value TEXT)")
                db.execute("INSERT INTO evidence VALUES(?)", ("preserve me",))
            dataset = root / "dataset"
            dataset.mkdir()
            feature = dataset / "features.bin"
            feature.write_bytes(b"features")
            (dataset / "dataset.json").write_text(json.dumps(dict(feature_shards=[dict(path=str(feature))])))
            (dataset / "schema.json").write_text("{}")
            archive = archive_storage.Archive(
                dict(
                    root=str(root / "archive"),
                    remote=str(root / "remote"),
                    local_remote=True,
                    min_free_bytes=0,
                    feature_cache_bytes=0,
                    maintenance=dict(runs=[str(run)], datasets=[str(dataset)], catalogues=[str(labels)]),
                )
            )
            storage_maintenance.maintain(archive)
            self.assertFalse(feature.exists())
            self.assertTrue(game.exists())
            self.assertTrue(weights.exists())
            self.assertTrue((root / "archive/maintenance-status.json").is_file())
            with archive.locked() as db:
                categories = {r[0] for r in db.execute("SELECT category FROM files")}
            self.assertTrue({"models", "games", "labels", "features", "other"} <= categories)
            storage_maintenance.maintain(archive)
            archive.hydrate(feature)
            self.assertEqual(feature.read_bytes(), b"features")
