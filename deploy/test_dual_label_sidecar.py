import json
from unittest.mock import patch

from test_tourney_analysis import AnalyzerFixture, a
import dual_label_sidecar as dual


class DualLabelTests(AnalyzerFixture):
    def test_reuses_old_label_and_adds_distinct_second_teacher(self):
        first = {'model_sha256': 'old', 'score': 12, 'ply': 2, 'budget_ms': 30000}
        moment = {'id': 'sample', 'center_ply': 2, 'game': 'game',
                  'game_hash': 'hash', 'searches': [first]}
        self.db.execute("INSERT INTO moments VALUES(?,0,'completed',?,NULL)",
                        (moment['id'], json.dumps(moment)))
        self.db.commit()
        config = {'budget_ms': 30000, 'old': {'label_teacher': {'name': 'ab', 'model': 'old-model'},
                          'models': {'old-model': {'sha256': 'old'}}},
                  'new_model': 'new-model', 'new_sha256': 'new', 'new_binding': {'sha256': 'new'}}
        second = {'model_sha256': 'new', 'score': -30, 'ply': 2, 'budget_ms': 30000}
        with patch.object(dual.a, 'search_position', return_value=second) as search:
            result = dual.paired_searches(config, moment, self.db)
        self.assertEqual(result['searches'], [first, second])
        self.assertEqual(search.call_count, 1)
        self.assertEqual(search.call_args.args[0]['label_teacher']['model'], 'new-model')
        self.assertEqual(search.call_args.args[0]['label_budget_ms'], 30000)
        saved = json.loads(self.db.execute('SELECT payload FROM moments').fetchone()[0])
        self.assertEqual(saved['searches'], [first])

    def test_missing_old_label_is_saved_before_second_search(self):
        moment = {'id': 'sample', 'center_ply': 2, 'game': 'game', 'game_hash': 'hash'}
        self.db.execute("INSERT INTO moments VALUES(?,0,'pending',?,NULL)",
                        (moment['id'], json.dumps(moment)))
        self.db.commit()
        config = {'budget_ms': 30000, 'old': {'label_teacher': {'name': 'ab', 'model': 'old-model'},
                          'models': {'old-model': {'sha256': 'old'}}},
                  'new_model': 'new-model', 'new_sha256': 'new', 'new_binding': {'sha256': 'new'}}
        def search(_, __, ___, db):
            if not json.loads(db.execute('SELECT payload FROM moments').fetchone()[0]).get('searches'):
                return {'model_sha256': 'old', 'budget_ms': 30000}
            raise RuntimeError('second search interrupted')
        with patch.object(dual.a, 'search_position', side_effect=search):
            with self.assertRaisesRegex(RuntimeError, 'interrupted'):
                dual.paired_searches(config, moment, self.db)
        saved = json.loads(self.db.execute('SELECT payload FROM moments').fetchone()[0])
        self.assertEqual([s['model_sha256'] for s in saved['searches']], ['old'])

    def test_old_ten_second_result_is_researched(self):
        moment = {'id': 'sample', 'center_ply': 2, 'game': 'game', 'game_hash': 'hash',
                  'searches': [{'model_sha256': 'old', 'budget_ms': 10000}]}
        self.db.execute("INSERT INTO moments VALUES(?,0,'completed',?,NULL)",
                        (moment['id'], json.dumps(moment)))
        self.db.commit()
        config = {'budget_ms': 30000, 'old': {'label_teacher': {'name': 'ab', 'model': 'old-model'},
                  'models': {'old-model': {'sha256': 'old'}}}, 'new_model': 'new-model',
                  'new_sha256': 'new', 'new_binding': {'sha256': 'new'}}
        results = [{'model_sha256': 'old', 'budget_ms': 30000},
                   {'model_sha256': 'new', 'budget_ms': 30000}]
        with patch.object(dual.a, 'search_position', side_effect=results) as search:
            result = dual.paired_searches(config, moment, self.db)
        self.assertEqual(search.call_count, 2)
        self.assertEqual(result['searches'], results)
