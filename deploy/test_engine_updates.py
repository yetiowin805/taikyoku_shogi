import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('rolling_api', Path(__file__).with_name('tourney_analysis.py'))
a = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = a
spec.loader.exec_module(a)
u = a.engine_updates


class Updates(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.run = Path(self.tmp.name)
        self.control = self.run/'analysis'
        self.control.mkdir()
        self.engine = self.run/'engine'
        self.engine.write_text('#!/bin/sh\n[ "$1" != tournament-game-protocol ] || echo 1\n')
        self.engine.chmod(0o755)
        self.bundle = u.snapshot(a, self.run, self.engine, self.engine, 'old')
        self.old_id = u.publish(a, self.run, self.bundle)
        self.model = self.run/'model.json'; self.model.write_text('{}')
        (self.control/'models').mkdir()
        binding = a.snapshot_model(self.model, self.model.read_bytes(), self.control)
        self.config = dict(rolling_engines=True, models={str(self.model):binding})
        a.atomic(self.control/'config.json', self.config)
        a.atomic(self.control/'manifest.json', {'entrants':[{'id':'normal','model':str(self.model)}, {'id':'frozen','model':'must-not-load','engine':'historical'}]})
        self.args = SimpleNamespace(rollback_build=None, compatible_speedup=True, engine=str(self.engine), analyzer=str(self.engine), revision='new')

    def tearDown(self): self.tmp.cleanup()

    def test_publish_rollback_and_rejection_leave_original_bytes_intact(self):
        old_bytes=Path(self.bundle['engine_bin']).read_bytes()
        self.engine.write_text(self.engine.read_text()+'# new binary\n')
        u.update(a,self.args,self.run)
        new=u.active(a,self.run)
        self.assertNotEqual(new['engine_sha256'],self.bundle['engine_sha256'])
        self.assertEqual(Path(self.bundle['engine_bin']).read_bytes(),old_bytes)
        self.args.rollback_build=self.old_id
        u.update(a,self.args,self.run)
        self.assertEqual(u.active(a,self.run),self.bundle)
        self.args.rollback_build=None
        self.model.write_text('{"changed":true}')
        with self.assertRaisesRegex(ValueError,'model changed'):u.update(a,self.args,self.run)
        self.assertEqual(u.active(a,self.run),self.bundle)

    def test_invalid_protocol_corruption_and_unenabled_run_fail_closed(self):
        self.engine.write_text('#!/bin/sh\necho incompatible\n')
        with self.assertRaisesRegex(ValueError,'protocol|support'):u.update(a,self.args,self.run)
        self.assertEqual(u.active(a,self.run),self.bundle)
        a.atomic(self.control/'config.json',dict(self.config,rolling_engines=False))
        with self.assertRaisesRegex(ValueError,'no rolling'):u.update(a,self.args,self.run)
        Path(self.bundle['engine_bin']).chmod(0o755)
        Path(self.bundle['engine_bin']).write_text('corrupt')
        with self.assertRaisesRegex(ValueError,'changed'):u.active(a,self.run)

    def test_game_analysis_remains_pinned_after_update(self):
        agent=dict(name='ab',engine_build=self.bundle)
        helper=a.analyzer_for_agent({},agent)
        with self.assertRaisesRegex(ValueError, "hash differs"):
            a.analyzer_for_agent({}, dict(agent,engine_sha256="wrong"))
        self.engine.write_text(self.engine.read_text()+'# new\n')
        u.update(a,self.args,self.run)
        self.assertEqual(a.analyzer_for_agent({},agent),helper)
        self.assertNotEqual(u.active(a,self.run)['analyzer_sha256'],helper['analyzer_sha256'])

    def test_teacher_request_switches_helper_and_cache_identity(self):
        def helper(score):
            self.engine.write_text("""#!/bin/sh\ncase "$1" in\n tournament-game-protocol) echo 1;;\n --validate-model|tournament-game-validate) exit 0;;\n *) echo '{"completed_depth":1,"score":%d}';;\nesac\n""" % score)
        helper(10)
        u.update(a,self.args,self.run)
        game=self.run/'game.json'
        a.atomic(game,dict(black=dict(name='ab',model=str(self.model)),white=dict(name='ab',model=str(self.model)),
                          moves=[dict(color='Black',eval=0)],result='Draw'))
        moment=dict(game=str(game),game_hash=a.file_digest(game))
        config=dict(self.config,run=str(self.run),label_teacher=dict(name='ab',model=str(self.model)),
                    analyzer_bin=self.bundle['analyzer_bin'],analyzer_sha256=self.bundle['analyzer_sha256'])
        db=a.connect(self.run)
        try:
            first=a.search_position(config,moment,1,db)
            helper(20)
            u.update(a,self.args,self.run)
            second=a.search_position(config,moment,1,db)
            self.assertEqual((first['score'],second['score']),(10,20))
            self.assertNotEqual(first['key'],second['key'])
            self.assertEqual(db.execute('SELECT count(*) FROM searches').fetchone()[0],2)
        finally:db.close()

    def test_resume_keeps_selection_and_legacy_teacher_binding(self):
        cpus=sorted(os.sched_getaffinity(0))[:4]
        if len(cpus)<4:self.skipTest('prepare requires four available CPUs')
        binaries=self.run/'target/release';binaries.mkdir(parents=True)
        for name in ('taikyoku_shogi','analyze_position'):shutil.copy2(self.engine,binaries/name)
        manifest=self.run/'manifest.json'
        a.atomic(manifest,{'entrants':[dict(id='normal',model=str(self.model))]})
        args=SimpleNamespace(action='start',manifest=str(manifest),cpus=','.join(map(str,cpus)),
                             depth=1,time_ms=10,rolling_engines=True,label_teacher='normal')
        original_run=subprocess.run
        def command(argv,**kwargs):
            if argv[0]=='git':return SimpleNamespace(stdout='fixture-revision\n')
            return original_run(argv,**kwargs)
        with patch.object(a,'ROOT',self.run),patch.object(a.subprocess,'run',side_effect=command):
            config=a.prepare(args,self.run)
            a.atomic(self.control/'config.json',config)
            a.atomic(self.run/'state.json',dict(entrants=a.read(manifest)['entrants'],depth=1,max_time_ms=10,format='knockout',slots=[]))
            self.engine.write_text(self.engine.read_text()+'# selected update\n')
            u.update(a,self.args,self.run)
            selected=u.active(a,self.run)
            args.action='resume'
            resumed=a.prepare(args,self.run)
            self.assertTrue(resumed['rolling_engines'])
            self.assertEqual(u.active(a,self.run),selected)
            self.assertEqual(resumed['analyzer_sha256'],config['analyzer_sha256'])

    def test_update_requires_explicit_compatibility_assertion(self):
        self.args.compatible_speedup=False
        with self.assertRaisesRegex(ValueError,'compatible-speedup'):u.update(a,self.args,self.run)
        self.assertEqual(u.active(a,self.run),self.bundle)

if __name__ == '__main__': unittest.main()
