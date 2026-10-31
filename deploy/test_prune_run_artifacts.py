import json
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("prune_run_artifacts.py")


class PruneRunArtifactsTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        run = self.run_dir = Path(self.tmp.name) / "run"
        workers = run / "analysis/game-workers"
        workers.mkdir(parents=True)
        timed = [{"move_number": 1, "iteration_timings": [{"depth": 1, "elapsed_us": 5, "completed": True}]}]
        self.game = run / "slot0001-a-vs-b-a-black.json"
        self.game.write_text(json.dumps({"game_id": "g1", "moves": timed}, indent=2))
        (run / "slot0002-a-vs-b-a-white.json").write_text(
            json.dumps({"game_id": "g2", "moves": [{"move_number": 1}]}, indent=2)
        )
        (run / "state.json").write_text(
            json.dumps(
                {
                    "slots": [
                        {"id": 1, "status": "done", "game_path": str(self.game)},
                        {"id": 2, "status": "done", "game_path": str(run / "slot0002-a-vs-b-a-white.json")},
                    ]
                }
            )
        )
        self.covered = workers / "slot-1.result.iterations.json"
        self.covered.write_text(json.dumps({"game_id": "g1", "moves": timed}))
        # The saved game lacks these timings (older coordinator): must be kept.
        self.uncovered = workers / "slot-2.result.iterations.json"
        self.uncovered.write_text(json.dumps({"game_id": "g2", "moves": timed}))
        with sqlite3.connect(run / "analysis/catalogue.sqlite") as db:
            db.execute("CREATE TABLE searches(key TEXT PRIMARY KEY, payload TEXT)")
            db.execute("INSERT INTO searches VALUES('done', '{}')")
            db.execute("CREATE TABLE moments(id TEXT PRIMARY KEY, payload TEXT)")
            db.execute("INSERT INTO moments VALUES('stored', '{}')")
        (run / "analysis/moments").mkdir()
        self.stored = run / "analysis/moments/stored.json"
        self.unknown = run / "analysis/moments/unknown.json"
        self.stored.write_text("{}")
        self.unknown.write_text("{}")
        self.done = run / "analysis/done.progress.json"
        self.pending = run / "analysis/pending.progress.json"
        self.done.write_text("{}")
        self.pending.write_text("{}")

    def tearDown(self):
        self.tmp.cleanup()

    def prune(self, *flags):
        subprocess.run(
            [sys.executable, str(SCRIPT), str(self.run_dir), *flags], check=True, capture_output=True, text=True
        )

    def test_dry_run_changes_nothing(self):
        self.prune()
        self.assertTrue(self.covered.exists() and self.done.exists() and self.stored.exists())

    def test_apply_removes_only_redundant_files_and_never_rewrites_games(self):
        # Game bytes identify games to label catalogues and NNUE datasets.
        before = self.game.read_bytes()
        self.prune("--apply")
        self.assertFalse(self.covered.exists())
        self.assertTrue(self.uncovered.exists())
        self.assertFalse(self.done.exists())
        self.assertTrue(self.pending.exists())
        self.assertFalse(self.stored.exists())
        self.assertTrue(self.unknown.exists())
        self.assertEqual(self.game.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
