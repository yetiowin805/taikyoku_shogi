import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("analysis", Path(__file__).with_name("tourney_analysis.py"))
a = importlib.util.module_from_spec(spec)
spec.loader.exec_module(a)


class AnalyzerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.run = Path(self.tmp.name)
        (self.run / "analysis/moments").mkdir(parents=True)
        a.STOPPING = False
        self.db = a.connect(self.run)

    def tearDown(self):
        self.db.close()
        self.tmp.cleanup()

    def game(self):
        return dict(game_id="fixture", result="Draw", black={"name": "ab", "model": "black"},
                    white={"name": "ab", "model": "white"},
                    moves=[dict(color="Black" if i % 2 == 0 else "White", eval=v)
                           for i, v in enumerate([0, 8000, 1000, 6999, -500, 7000, 1200])])

    def test_threshold_sign_same_player_and_boundaries(self):
        results = list(a.candidates(self.game()))
        self.assertEqual([r["change"] for r in results], [1000, -1001, -1500, 1700])
        self.assertEqual(results[0]["plies"], [1, 2, 3, 4, 5])
        self.assertEqual(results[-1]["plies"], [3, 4, 5, 6, 7])
        game = self.game()
        game["moves"][2]["color"] = "White"
        game["moves"][3]["eval"] = None
        self.assertFalse(any(x["later_ply"] in (3, 4) for x in a.candidates(game)))

    def test_scan_only_completed_incremental_and_restart(self):
        game = self.run / "game.json"
        a.atomic(game, self.game())
        a.atomic(self.run / "state.json", {"slots": [
            {"id": 1, "status": "done", "game_path": str(game)},
            {"id": 2, "status": "running", "game_path": "missing"}]})
        a.scan(self.db, self.run)
        a.scan(self.db, self.run)
        self.assertEqual(self.db.execute("select count(*) from moments").fetchone()[0], 4)
        self.db.execute("update moments set status='running'")
        self.db.commit()
        self.db.close()
        self.db = a.connect(self.run)
        self.assertEqual(self.db.execute("select count(*) from moments where status='pending'").fetchone()[0], 4)

    def test_original_agent_budget_cache_and_watchdog(self):
        game = self.game()
        models = {}
        for side in ("black", "white"):
            path = self.run / (side + ".json")
            path.write_text("{}")
            game[side]["model"] = str(path)
            models[str(path)] = {"sha256": a.digest(path.read_bytes()), "snapshot": str(path)}
        path = self.run / "game.json"
        a.atomic(path, game)
        engine = self.run / "engine"
        engine.write_text("""#!/usr/bin/env python3
import json,sys,time
print(json.dumps(dict(completed_depth=2,score=37,best_move='test',args=sys.argv[1:])),flush=True)
if 'watchdog' in sys.argv[0]: time.sleep(10)
""")
        engine.chmod(0o755)
        cfg = dict(run=str(self.run), models=models, analyzer_bin=str(engine), analyzer_sha256="fake")
        moment = {"game": str(path), "game_hash": a.digest(path.read_bytes())}
        first = a.search_position(cfg, moment, 2, self.db)
        self.assertEqual(first["agent"], game["white"])
        self.assertEqual(first["args"][-2:], ["64", "30000"])
        engine.unlink()
        self.assertEqual(a.search_position(cfg, moment, 2, self.db), first)
        # Future records target completed depth + 1, never the configured ceiling.
        game["moves"][1]["completed_depth"] = 3
        a.atomic(path, game)
        moment["game_hash"] = a.digest(path.read_bytes())
        watchdog = self.run / "watchdog"
        watchdog.write_text("#!/usr/bin/env python3\nimport json,time\n"
                            "print(json.dumps(dict(completed_depth=2,score=99,best_move='x')),flush=True)\n"
                            "time.sleep(10)\n")
        watchdog.chmod(0o755)
        cfg["analyzer_bin"] = str(watchdog)
        old = a.SEARCH_HARD_SECONDS
        try:
            a.SEARCH_HARD_SECONDS = .2
            result = a.search_position(cfg, moment, 2, self.db)
        finally:
            a.SEARCH_HARD_SECONDS = old
        self.assertTrue(result["hard_timeout"])
        self.assertEqual(result["completed_depth"], 2)
        self.assertEqual(result["target_depth"], 4)
        self.assertEqual(result["budget_ms"], 300000)
        Path(game["white"]["model"]).write_text('{"changed":true}')
        with self.assertRaisesRegex(ValueError, "model changed"):
            a.search_position(cfg, moment, 2, self.db)


    def test_batch_reuses_worker_and_rejects_uncorrelated_events(self):
        engine = self.run / "batch"
        engine.write_text("""#!/usr/bin/env python3
import json,sys
for line in sys.stdin:
    r=json.loads(line)
    identity=dict(version=1,id='wrong' if r['ply']==99 else r['id'])
    print(json.dumps(dict(identity,event='iteration',completed_depth=2,score=r['ply'])),flush=True)
    print(json.dumps(dict(identity,event='complete')),flush=True)
""")
        engine.chmod(0o755)
        worker = a.BatchWorker(dict(run=str(self.run), analyzer_bin=str(engine)))
        self.addCleanup(worker.close)
        progress = []
        def search(ply):
            return worker.search("game", "hash", ply, "model", 2, 100, progress.append)
        self.assertEqual(search(3), ({"completed_depth": 2, "score": 3}, False))
        pid = worker.proc.pid
        self.assertEqual(search(1)[0]["score"], 1)
        self.assertEqual(worker.proc.pid, pid)
        with self.assertRaisesRegex(ValueError, "uncorrelated"):
            search(99)
        self.assertIsNone(worker.proc)
        self.assertEqual(search(4)[0]["score"], 4)
        self.assertNotEqual(worker.proc.pid, pid)
        self.assertEqual([p["score"] for p in progress], [3, 1, 4])

    def test_batch_partial_line_watchdog_preserves_completed_iteration(self):
        engine = self.run / "partial"
        engine.write_text("""#!/usr/bin/env python3
import json,sys,time
r=json.loads(sys.stdin.readline())
identity=dict(version=1,id=r['id'],event='iteration')
for depth in (2,1):
    print(json.dumps(dict(identity,completed_depth=depth,score=depth)),flush=True)
sys.stdout.write('{');sys.stdout.flush();time.sleep(10)
""")
        engine.chmod(0o755)
        worker = a.BatchWorker(dict(run=str(self.run), analyzer_bin=str(engine)))
        self.addCleanup(worker.close)
        old = a.SEARCH_HARD_SECONDS
        progress = []
        try:
            a.SEARCH_HARD_SECONDS = .3
            best, timeout = worker.search("game", "hash", 1, "model", 8, 100, progress.append)
        finally:
            a.SEARCH_HARD_SECONDS = old
        self.assertTrue(timeout)
        self.assertEqual(best, {"completed_depth": 2, "score": 2})
        self.assertEqual(progress, [best])
        self.assertIsNone(worker.proc)


if __name__ == "__main__":
    unittest.main()
