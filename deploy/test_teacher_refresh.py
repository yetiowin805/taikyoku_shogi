import json
import contextlib
from unittest.mock import patch

from test_tourney_analysis import AnalyzerFixture
import dual_label_sidecar as dual
import teacher_refresh as refresh


class TeacherRefreshTests(AnalyzerFixture):
    def test_switch_requires_verified_backup_and_publishes_compressed_rollback(self):
        a = dual.a
        model = self.run/'teacher.json'; model.write_text('{"name":"new teacher"}')
        config_path = self.run/'analysis/dual-label-config.json'
        config_path.write_text(json.dumps(dict(old={}, new_sha256='old', analyzer_bin='fake-helper')))
        original = config_path.read_bytes()
        with contextlib.closing(a.connect(self.run, 'training-labels.sqlite')) as db:
            db.executescript('CREATE TABLE dual_paired(id TEXT PRIMARY KEY); CREATE TABLE dual_failures(id TEXT PRIMARY KEY,error TEXT,updated REAL);')
        with patch.object(a, 'validate_source'), patch.object(a.subprocess, 'run'), \
                patch.object(a, 'snapshot_model', return_value={'snapshot':'pinned'}):
            with patch.object(refresh.artifact_storage, 'teacher_backup', side_effect=OSError('disk full')):
                with self.assertRaisesRegex(OSError, 'disk full'):
                    refresh.replace(a, self.run, model)
            self.assertEqual(config_path.read_bytes(), original)
            report = refresh.replace(a, self.run, model)
        backup = self.run/'analysis/teacher-backups'
        saved = next(p for p in backup.iterdir() if p.name.isdigit())
        self.assertTrue((saved/'training-labels.sqlite.gz').is_file())
        self.assertEqual((saved/'dual-label-config.json').read_bytes(), original)
        self.assertTrue(report['changed'])
        self.assertEqual(a.read(config_path)['new_teacher_id'], 'new teacher')

    def setup_catalogue(self):
        self.db.executescript('CREATE TABLE dual_paired(id TEXT PRIMARY KEY); CREATE TABLE dual_failures(id TEXT PRIMARY KEY,error TEXT,updated REAL);')

    def add(self, moment):
        self.db.execute("INSERT INTO moments VALUES(?,0,'completed',?,NULL)", (moment['id'], json.dumps(moment)))
        self.db.execute('INSERT INTO dual_paired VALUES(?)', (moment['id'],))
        self.db.commit()

    def test_selection_is_bounded_deterministic_and_spans_outcomes(self):
        self.setup_catalogue()
        for game, outcome in [('win', 'BlackWins'), ('loss', 'WhiteWins'), ('draw', 'Draw')]:
            for i in range(40):
                self.add(dict(id=f'{game}-{i}', game_hash=game, game_result=outcome,
                              center_ply=i*10+1, searches=[dict(score=i*100, model_sha256='old')]))
        ids = refresh.select_backfill(self.db, 'new')
        self.assertEqual(ids, refresh.select_backfill(self.db, 'new'))
        for game in ('win', 'loss', 'draw'):
            selected = [int(x.split('-')[1]) for x in ids if x.startswith(game)]
            self.assertLessEqual(len(selected), 8)
            self.assertTrue(any(i < 10 for i in selected))
            self.assertTrue(any(i >= 30 for i in selected))

    def test_requeue_preserves_labels_and_does_not_requeue_completed_refresh(self):
        self.setup_catalogue()
        old = dict(model_sha256='old', score=42)
        self.add(dict(id='m', searches=[old]))
        config = dict(teacher_refresh=dict(id='new', ids=['m']))
        refresh.apply_backfill(self.db, config)
        self.assertIsNone(self.db.execute("SELECT id FROM dual_paired WHERE id='m'").fetchone())
        self.assertEqual(json.loads(self.db.execute('SELECT payload FROM moments').fetchone()[0])['searches'], [old])
        self.db.execute("INSERT INTO dual_paired VALUES('m')"); self.db.commit()
        refresh.apply_backfill(self.db, config)
        self.assertIsNotNone(self.db.execute("SELECT id FROM dual_paired WHERE id='m'").fetchone())

    def test_second_search_interruption_archives_old_default_immediately(self):
        self.setup_catalogue()
        first = dict(model_sha256='first', budget_ms=30000, key='a')
        previous = dict(model_sha256='2048', budget_ms=30000, key='b')
        moment = dict(id='m', center_ply=2, searches=[first, previous])
        self.add(moment)
        config = dict(old=dict(label_teacher=dict(name='ab', model='first'), models={'first': {'sha256': 'first'}}),
                      budget_ms=30000, new_model='seeds', new_sha256='seeds', new_binding={'sha256': 'seeds'}, new_teacher_id='SEEDS2')
        with patch.object(dual.a, 'search_position', side_effect=InterruptedError):
            with self.assertRaises(InterruptedError):dual.paired_searches(config, moment, self.db)
        saved = json.loads(self.db.execute('SELECT payload FROM moments').fetchone()[0])
        self.assertEqual(saved['previous_teacher_searches'], [previous])
        new = dict(model_sha256='seeds', budget_ms=30000, key='c')
        with patch.object(dual.a, 'search_position', return_value=new) as search:
            result = dual.paired_searches(config, saved, self.db)
        self.assertEqual(search.call_count, 1)
        self.assertEqual(search.call_args.args[0]['label_teacher_id'], 'SEEDS2')
        self.assertEqual(result['searches'], [first, new])
        self.assertEqual(result['previous_teacher_searches'], [previous])

    def test_same_winner_and_default_search_only_once(self):
        self.setup_catalogue()
        moment = dict(id='m', center_ply=2, searches=[])
        self.add(moment)
        config = dict(old=dict(label_teacher=dict(name='ab', model='seeds'), models={'seeds': {'sha256': 'seeds'}}),
                      budget_ms=30000, new_model='seeds', new_sha256='seeds', new_binding={'sha256': 'seeds'})
        result = dict(model_sha256='seeds', budget_ms=30000, key='a')
        with patch.object(dual.a, 'search_position', return_value=result) as search:
            self.assertEqual(dual.paired_searches(config, moment, self.db)['searches'], [result])
        self.assertEqual(search.call_count, 1)
