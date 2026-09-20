import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("analysis", Path(__file__).with_name("tourney_analysis.py"))
a = importlib.util.module_from_spec(spec)
spec.loader.exec_module(a)


class AnalyzerFixture(unittest.TestCase):
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

class AnalyzerTests(AnalyzerFixture):
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

class HistoricalAndCarryTests(AnalyzerFixture):
    def test_historical_binding_and_carried_catalogue(self):
        game = self.game()
        model = self.run / 'model.json'; model.write_text('{}')
        model_map = {str(model): {'sha256': a.digest(model.read_bytes()), 'snapshot': str(model)}}
        engine = self.run / 'historical'; engine.write_text('old search binary')
        helper = self.run / 'old-helper'
        helper.write_text('#!/usr/bin/env python3\nimport json,sys\n'
                          'print(json.dumps(dict(completed_depth=2,score=123,args=sys.argv[1:])))\n')
        helper.chmod(0o755)
        entry = dict(engine_sha256=a.digest(engine.read_bytes()), analyzer_bin=str(helper),
                     analyzer_sha256=a.digest(helper.read_bytes()))
        for side in ('black', 'white'):
            game[side].update(model=str(model), engine=str(engine))
        old_run = self.run/'old'; old_run.mkdir()
        path = old_run/'game.json'; a.atomic(path, game)
        source = dict(run=str(old_run), models=model_map, historical_engines={str(engine): entry},
                      analyzer_bin=str(helper), analyzer_sha256=entry['analyzer_sha256'])
        cfg = dict(run=str(self.run), models={}, analyzer_bin='MUST_NOT_RUN_CURRENT_ENGINE',
                   analyzer_sha256='new', analysis_sources=[source])
        moment = dict(game=str(path), game_hash=a.digest(path.read_bytes()))
        result = a.search_position(cfg, moment, 1, self.db)
        self.assertEqual(result['score'], 123)
        self.assertEqual(result['args'][-1], '--allow-historical')
        self.assertEqual(result['analyzer_sha256'], entry['analyzer_sha256'])
        self.assertEqual(a.search_position(cfg, moment, 1, self.db), result)
        self.assertEqual(self.db.execute('select count(*) from searches').fetchone()[0], 1)
        # Old ordinary games also use the old helper, without bypassing a historical guard.
        for side in ('black', 'white'): game[side].pop('engine')
        a.atomic(path, game); moment['game_hash'] = a.digest(path.read_bytes())
        result = a.search_position(cfg, moment, 1, self.db)
        self.assertNotEqual(result['args'][-1], '--allow-historical')
        helper.write_text('changed')
        with self.assertRaisesRegex(ValueError, 'changed'):
            a.validate_source(source)

    def test_historical_pair_metadata_and_resume(self):
        binary = self.run/'frozen'; binary.write_text('engine'); binary.chmod(0o755)
        helper = self.run/'frozen.analyze_position'; helper.write_text('helper'); helper.chmod(0o755)
        meta = self.run/'frozen.meta'
        meta.write_text(f'rev=old\nengine_sha256={a.digest(binary.read_bytes())}\nanalyzer_sha256={a.digest(helper.read_bytes())}\n')
        entry = a.historical_pair(binary, {})
        previous = {'historical_engines': {str(binary): entry}}
        self.assertEqual(a.historical_pair(binary, previous), entry)
        helper.write_text('different helper')
        with self.assertRaisesRegex(ValueError, 'changed'):
            a.historical_pair(binary, previous)
        with self.assertRaisesRegex(ValueError, 'metadata mismatch'):
            a.historical_pair(binary, {})

    def test_prepare_carries_results_and_resumes_engine_bindings(self):
        from types import SimpleNamespace
        from unittest.mock import patch
        for name in ('taikyoku_shogi', 'analyze_position'):
            binary = self.run/'target/release'/name
            binary.parent.mkdir(parents=True, exist_ok=True)
            binary.write_text('#!/bin/sh\nexit 0\n'); binary.chmod(0o755)
        model = self.run/'model.json'; model.write_text('{}')
        historical = self.run/'historical'; historical.write_text('#!/bin/sh\nexit 0\n'); historical.chmod(0o755)
        helper = self.run/'historical.analyze_position'; helper.write_bytes(historical.read_bytes()); helper.chmod(0o755)
        (self.run/'historical.meta').write_text(f'rev=old\nengine_sha256={a.digest(historical.read_bytes())}\nanalyzer_sha256={a.digest(helper.read_bytes())}\n')
        source_config = dict(run=str(self.run), models={str(model): {'sha256':a.digest(model.read_bytes()),'snapshot':str(model)}},
                             analyzer_bin=str(helper), analyzer_sha256=a.digest(helper.read_bytes()))
        a.atomic(self.run/'analysis/config.json', source_config)
        a.atomic(self.run/'state.json', {'slots':[]})
        payload = {'id':'completed', 'status':'completed', 'searches':[{'score':7}]}
        self.db.execute("INSERT INTO moments VALUES('completed',1000,'completed',?,NULL)", (json.dumps(payload),))
        self.db.execute("INSERT INTO searches VALUES('cached','{}')"); self.db.commit()
        manifest = self.run/'field.json'
        a.atomic(manifest, {'entrants':[{'id':'old','model':str(model),'engine':str(historical)}, {'id':'new','model':str(model)}]})
        new = self.run/'next'
        args = SimpleNamespace(action='start',manifest=str(manifest),cpus='0,1,2,3',depth=8,time_ms=3000,carry_analysis_from=str(self.run))
        with patch.object(a, 'ROOT', self.run), patch.object(a.os, 'sched_getaffinity', return_value={0,1,2,3}):
            config = a.prepare(args, new)
            entry = a.read(new/'analysis/manifest.json')['entrants'][0]
            selected = a.analyzer_for_agent(config, entry)
            self.assertEqual(Path(selected['analyzer_bin']).parent, new/'analysis/bin')
            a.atomic(new/'analysis/config.json', config)
            a.atomic(new/'state.json', {'entrants': a.read(new/'analysis/manifest.json')['entrants'], 'depth':8, 'max_time_ms':3000})
            args.action='resume'; args.carry_analysis_from=None
            resumed = a.prepare(args, new)
        self.assertEqual(resumed['analysis_sources'], config['analysis_sources'])
        self.assertEqual(a.read(new/'analysis/moments/completed.json'), payload)
        with a.sqlite3.connect(new/'analysis/catalogue.sqlite') as db:
            self.assertEqual(db.execute('select count(*) from searches').fetchone()[0], 1)
        self.assertEqual(self.db.execute('select count(*) from searches').fetchone()[0], 1)


if __name__ == "__main__":
    unittest.main()

class NnueSnapshotTests(unittest.TestCase):
    def test_dependency_pinned_and_source_change_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);control=root/'analysis';(control/'models').mkdir(parents=True)
            blob=root/'weights.bin';blob.write_bytes(b'test network bytes')
            source=root/'agent.json';source.write_text(json.dumps({'weights':{'nnue':{'file':blob.name,'sha256':a.file_digest(blob),'width':512}}}))
            binding=a.snapshot_model(source,source.read_bytes(),control)
            snapshot=json.loads(Path(binding['snapshot']).read_text())
            self.assertEqual(Path(snapshot['weights']['nnue']['file']).read_bytes(),blob.read_bytes())
            self.assertNotEqual(binding['sha256'],binding['snapshot_sha256'])
            a.validate_model_binding(source,binding)
            blob.write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError,'artifact changed'):
                a.validate_model_binding(source,binding)
