import unittest
from types import SimpleNamespace
from unittest.mock import patch
import tourney_analysis as a


class ClockTests(unittest.TestCase):
    def test_explicit_saved_and_legacy_clocks(self):
        args = SimpleNamespace(initial_time_ms=900000, increment_ms=5000, time_ms=None)
        clock = a.game_clock(args, None)
        self.assertEqual(clock, dict(initial_ms=900000, increment_ms=5000))
        args.initial_time_ms = args.increment_ms = None
        self.assertEqual(a.game_clock(args, {'time_control': clock}), clock)
        self.assertIsNone(a.game_clock(args, {'max_time_ms': 3000}))
        args.time_ms = 3000
        with self.assertRaisesRegex(ValueError, 'combine'):
            a.game_clock(args, {'time_control': clock})
        args.time_ms = None
        args.initial_time_ms = 900000
        with self.assertRaisesRegex(ValueError, 'together'):
            a.game_clock(args, None)
        args.increment_ms = 0
        with self.assertRaisesRegex(ValueError, 'invalid'):
            a.game_clock(args, None)

    def test_capability_must_be_explicit(self):
        with patch.object(a.engine_updates.subprocess, 'run') as run:
            run.return_value.stdout = 'usage: old binary'
            with self.assertRaisesRegex(ValueError, 'support Fischer'):
                a.engine_updates.require_clock_support('/fake')
            run.return_value.stdout = '1\n'
            a.engine_updates.require_clock_support('/fake')

    def test_launcher_switch_and_resume_use_clock_flags(self):
        import tempfile
        from pathlib import Path
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name in ('taikyoku_shogi', 'analyze_position'):
                binary = root/'target/release'/name
                binary.parent.mkdir(parents=True, exist_ok=True)
                binary.write_text('#!/bin/sh\n[ "$1" != tournament-game-clock-protocol ] || echo 1\nexit 0\n')
                binary.chmod(0o755)
            model = root/'model.json'; model.write_text('{}')
            manifest = root/'field.json'
            entrants = [dict(id='a', model=str(model)), dict(id='b', model=str(model))]
            a.atomic(manifest, dict(entrants=entrants))
            args = SimpleNamespace(action='start', manifest=str(manifest), cpus='0,1,2,3',
                                   depth=None, time_ms=None, initial_time_ms=900000, increment_ms=5000)
            run = root/'run'
            with patch.object(a, 'ROOT', root), patch.object(a.os, 'sched_getaffinity', return_value={0,1,2,3}):
                config = a.prepare(args, run)
                self.assertNotIn('--time-ms', config['command'])
                self.assertEqual(config['command'][config['command'].index('--depth')+1], '64')
                self.assertIn('900000', config['command'])
                a.atomic(run/'analysis/config.json', config)
                a.atomic(run/'state.json', dict(entrants=entrants, depth=9, time_control=config['time_control']))
                config['rolling_engines'] = True
                a.atomic(run/'analysis/config.json', config)
                (root/'target/release/analyze_position').write_text('#!/bin/sh\nexit 0\n# new helper\n')
                args.action = 'resume'; args.initial_time_ms = args.increment_ms = None
                with patch.object(a.engine_updates, 'active', return_value={'engine_bin': str(root/'target/release/taikyoku_shogi')}), patch.object(a.subprocess, 'run', return_value=SimpleNamespace(stdout='1\n')):
                    resumed = a.prepare(args, run)
                self.assertEqual(resumed['analyzer_sha256'], config['analyzer_sha256'])
                self.assertEqual(resumed['time_control'], config['time_control'])
                self.assertEqual(resumed['command'][resumed['command'].index('--depth')+1], '9')
