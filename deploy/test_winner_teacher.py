import json
from pathlib import Path
from unittest.mock import patch

from test_tourney_analysis import AnalyzerFixture, a
import dual_label_sidecar as dual
import winner_teacher as w


class WinnerTeacherTests(AnalyzerFixture):
    def fixture(self):
        game = self.game()
        game['result'] = 'BlackWins'
        game['moves'] = [dict(color='Black' if i % 2 == 0 else 'White', eval=0) for i in range(120)]
        game['moves'][51]['eval'] = 2000  # White gets worse, Black absolute.
        game['moves'][99]['eval'] = 1000000
        models = {}
        for side in ('black', 'white'):
            p = self.run / (side + '.json')
            a.atomic(p, {'weights': {'nnue': {'width': 512}} if side == 'white' else {}})
            game[side]['model'] = str(p)
            models[str(p)] = {'sha256': a.file_digest(p), 'snapshot': str(p)}
        self.db.executescript('CREATE TABLE dual_paired(id TEXT PRIMARY KEY); CREATE TABLE dual_failures(id TEXT PRIMARY KEY,error TEXT,updated REAL);')
        return game, {'old': {'models': models}}

    def test_pre_event_positions_spacing_quota_and_split(self):
        game, _ = self.fixture()
        samples = w.candidates(game)
        targets = [s for s in samples if s['selection'] != 'winner_representative']
        self.assertEqual({s['center_ply'] for s in targets}, {36,44,48,50,84,92,96,98})
        self.assertLessEqual(len(samples), 32)
        self.assertEqual(len({s['center_ply'] for s in samples}), len(samples))
        self.assertTrue(all(s['center_ply'] < s['event_ply'] for s in targets))
        self.assertEqual(samples, w.candidates(game))
        self.assertEqual(samples[0]['split_group'], a.training_labels.candidates(game)[0]['split_group'])
        # Color reversal with score reversal has exactly the same sampled plies.
        game['result'] = 'WhiteWins'
        for m in game['moves']:
            m['color'] = 'White' if m['color'] == 'Black' else 'Black'
            m['eval'] *= -1
        self.assertEqual([s['center_ply'] for s in targets], [s['center_ply'] for s in w.candidates(game) if s['selection'] != 'winner_representative'])

    def test_boundaries_missing_scores_and_adjacent_events(self):
        game, _ = self.fixture()
        game['moves'] = game['moves'][:8]
        game['moves'][3]['eval'] = 2000
        game['moves'][5]['eval'] = 4000
        game['moves'][7]['eval'] = 1000000
        game['moves'][1]['eval'] = None
        samples = w.candidates(game)
        self.assertTrue(all(1 <= s['center_ply'] <= 8 for s in samples))
        self.assertTrue(all(s['center_ply'] != 2 for s in samples))
        self.assertLessEqual(len({s['event_ply'] for s in samples if 'event_ply' in s}), 1)

    def test_teacher_selection_uses_contents_and_retains_historical_binding(self):
        game, config = self.fixture()
        game['black']['engine'] = 'frozen-engine'
        self.assertEqual(w.winner_agent(config, game)['engine'], 'frozen-engine')
        game['result'] = 'WhiteWins'
        self.assertIsNone(w.winner_agent(config, game))
        game['result'] = 'Draw'
        self.assertIsNone(w.winner_agent(config, game))

    def test_backfill_reuses_position_preserves_results_and_is_idempotent(self):
        game, config = self.fixture()
        p = self.run / 'game.json';a.atomic(p, game)
        a.atomic(self.run / 'state.json', {'slots':[{'id':1,'status':'done','game_path':str(p)}]})
        a.scan(self.db,self.run,labels=True)
        oldids={r['id'] for r in self.db.execute('SELECT id FROM moments')}
        # Ensure an existing completed targeted position will be migrated.
        moment=dict(id='existing',center_ply=98,game_hash=a.file_digest(p),searches=[{'model_sha256':'old','budget_ms':30000}])
        self.db.execute("INSERT INTO moments VALUES('existing',0,'completed',?,NULL)",(json.dumps(moment),))
        self.db.execute("INSERT INTO dual_paired VALUES('existing')");self.db.commit()
        w.scan(a,self.db,self.run,config)
        count=self.db.execute('SELECT count(*) FROM moments').fetchone()[0]
        w.scan(a,self.db,self.run,config)
        self.assertEqual(count,self.db.execute('SELECT count(*) FROM moments').fetchone()[0])
        updated=json.loads(self.db.execute("SELECT payload FROM moments WHERE id='existing'").fetchone()[0])
        self.assertEqual(updated['searches'],moment['searches'])
        self.assertIn('winner_teacher',updated)
        self.assertFalse(self.db.execute("SELECT 1 FROM dual_paired WHERE id='existing'").fetchone())
        self.assertTrue(oldids.issubset({r['id'] for r in self.db.execute('SELECT id FROM moments')}))

    def test_pair_reuses_nnue_archives_old_teacher_and_pins_historical_helper(self):
        game, config = self.fixture()
        agent={**game['black'],'engine':'historical'}
        old=dict(model_sha256='old512',budget_ms=30000,key='oldkey')
        nn=dict(model_sha256='new',budget_ms=30000,key='nnkey')
        moment=dict(id='m',center_ply=2,winner_teacher=agent,searches=[old,nn])
        self.db.execute("INSERT INTO moments VALUES('m',0,'pending',?,NULL)",(json.dumps(moment),));self.db.commit()
        config.update(budget_ms=30000,new_model='new',new_sha256='new',new_binding={'sha256':'new'})
        config['old'].update(label_teacher={'name':'ab','model':'old'},rolling_engines=True)
        hc=dict(model_sha256=config['old']['models'][agent['model']]['sha256'],budget_ms=30000,key='hckey')
        with patch.object(dual.a,'search_position',return_value=hc) as search:
            result=dual.paired_searches(config,moment,self.db)
        self.assertEqual(search.call_count,1)
        self.assertFalse(search.call_args.args[0]['rolling_engines'])
        self.assertEqual(result['searches'],[hc,nn])
        self.assertEqual(result['previous_teacher_searches'],[old])

    def test_four_episodes_and_sixteen_representatives(self):
        game, _ = self.fixture()
        game['moves'] = [dict(color='Black' if i % 2 == 0 else 'White', eval=0) for i in range(800)]
        for i in (101, 201, 301, 401, 501, 601):
            game['moves'][i]['eval'] = 2000
        samples = w.candidates(game)
        targeted = [s for s in samples if 'event_ply' in s]
        self.assertEqual(len(targeted), 16)
        self.assertEqual(len({s['event_ply'] for s in targeted}), 4)
        self.assertEqual({s['offset_plies'] for s in targeted}, {-16, -8, -4, -2})
        self.assertEqual(len(samples), 32)
        self.assertTrue(all(s['selection_quota'] == 32 for s in samples))

    def test_new_policy_backfills_old_ledger_without_duplicate_positions(self):
        game, config = self.fixture()
        p = self.run / 'game.json'; a.atomic(p, game)
        a.atomic(self.run / 'state.json', {'slots':[{'id':1,'status':'done','game_path':str(p)}]})
        with patch.object(w, 'POLICY', 'winner-teacher-v1'):
            w.scan(a, self.db, self.run, config)
        before = {json.loads(r['payload'])['center_ply']:r['id'] for r in self.db.execute('SELECT * FROM moments')}
        w.scan(a, self.db, self.run, config)
        rows = list(self.db.execute('SELECT * FROM moments'))
        positions = [json.loads(r['payload'])['center_ply'] for r in rows]
        self.assertEqual(len(positions), len(set(positions)))
        for r in rows:
            ply = json.loads(r['payload'])['center_ply']
            if ply in before:
                self.assertEqual(r['id'], before[ply])
        self.assertEqual(self.db.execute('SELECT count(*) FROM winner_scanned').fetchone()[0], 2)
