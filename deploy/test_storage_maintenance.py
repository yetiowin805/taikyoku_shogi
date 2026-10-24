import json
import sqlite3
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

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
            complete = json.loads((root / "remote/maintenance-complete.json").read_text())
            self.assertEqual(complete["index_sha256"], archive_storage.digest(Path(complete["index_remote"])))
            committed = (root / "remote/maintenance-complete.json").read_bytes()
            later = run / "slot2.json"
            later.write_text(json.dumps(dict(result="Draw", moves=[])))
            state = json.loads((run / "state.json").read_text())
            state["slots"].append(dict(status="done", game_path=str(later)))
            (run / "state.json").write_text(json.dumps(state))
            real_copy = archive.copy

            def fail_commit(source, destination, immutable=False):
                if destination.endswith("/maintenance-complete.json"):
                    raise RuntimeError("commit interrupted")
                return real_copy(source, destination, immutable)

            with patch.object(archive, "copy", side_effect=fail_commit):
                with self.assertRaisesRegex(RuntimeError, "commit interrupted"):
                    storage_maintenance.maintain(archive)
            self.assertEqual((root / "remote/maintenance-complete.json").read_bytes(), committed)
            recovered = archive_storage.Archive(dict(archive.config, root=str(root / "recovered")), import_legacy=False)
            recovered.recover_index()
            with recovered.locked() as db:
                self.assertTrue(db.execute("SELECT 1 FROM files WHERE path=?", (str(feature),)).fetchone())
                self.assertIsNone(db.execute("SELECT 1 FROM files WHERE path=?", (str(later),)).fetchone())
            archive.hydrate(feature)
            self.assertEqual(feature.read_bytes(), b"features")

    def test_forward_model_bindings_are_dependencies_before_admission(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original, snapshot, weights = [root / name for name in ("model.json", "snapshot.json", "weights.nnue")]
            for path in (original, snapshot, weights):
                path.write_text("frozen")
            value = dict(
                models={
                    str(original): dict(
                        snapshot=str(snapshot), artifacts=[dict(original=str(weights), snapshot=str(weights))]
                    )
                }
            )
            self.assertEqual(set(storage_maintenance.binding_files(value, root)), {original, snapshot, weights})
