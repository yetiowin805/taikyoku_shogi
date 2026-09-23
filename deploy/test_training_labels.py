import copy
import unittest
from test_tourney_analysis import AnalyzerFixture, a
import training_labels as labels


class SamplingTests(unittest.TestCase):
    def game(self):
        return dict(start={'kind': 'opening'}, result='BlackWins',
                    black={'name':'ab'}, white={'name':'ab'},
                    moves=[dict(color='Black' if i%2==0 else 'White',
                                eval=(i//2%2)*2000, from_file=i%36,
                                from_rank=3,to_file=1,to_rank=4,data='Standard',promoted=False)
                           for i in range(200)])

    def test_deterministic_bounded_stratified_and_disjoint(self):
        g=self.game(); rows=labels.candidates(g)
        self.assertEqual(rows,labels.candidates(g))
        self.assertLessEqual(len(rows),20)
        self.assertEqual(len({r['center_ply'] for r in rows}),len(rows))
        self.assertEqual(sum(r['selection']=='representative' for r in rows),12)
        self.assertEqual({r['selection'] for r in rows},
                         {'representative','evaluation_swing','decisive_ending'})
        self.assertTrue(all(r['plies']==[r['center_ply']] for r in rows))
        other=copy.deepcopy(g);other['moves'][0]['eval']=700;other['black']['model']='different'
        self.assertEqual(rows[0]['split_group'],labels.candidates(other)[0]['split_group'])
        other['moves'][0]['to_file']=2
        self.assertNotEqual(rows[0]['split_group'],labels.candidates(other)[0]['split_group'])

    def test_bad_scores_draws_historical_and_short_games(self):
        g=self.game();g['result']='Draw'
        g['moves'][0]['eval']=True;g['moves'][1]['eval']=float('nan');g['moves'][2]['eval']=1000000
        rows=labels.candidates(g)
        self.assertTrue(all(r['center_ply']>3 for r in rows))
        self.assertFalse(any(r['selection']=='decisive_ending' for r in rows))
        g['moves']=g['moves'][3:4]
        self.assertEqual(len(labels.candidates(g)),1)
        g['white']['engine']='old'
        self.assertEqual(labels.candidates(g),[])


class LabelSearchTests(AnalyzerFixture):
    def test_teacher_fixed_budget_iterations_and_independent_catalogue(self):
        g=self.game();model=self.run/'teacher.json';model.write_text('{}')
        path=self.run/'game.json';a.atomic(path,g)
        helper=self.run/'helper';helper.write_text('#!/usr/bin/env python3\nimport json,sys\n'
            'for d in (1,2): print(json.dumps(dict(completed_depth=d,score=d,args=sys.argv[1:])),flush=True)\n')
        helper.chmod(0o755)
        teacher={'name':'ab','model':str(model)}
        cfg=dict(run=str(self.run),label_teacher=teacher,models={str(model):
            {'sha256':a.digest(model.read_bytes()),'snapshot':str(model)}},
            analyzer_bin=str(helper),analyzer_sha256='fixture')
        result=a.search_position(cfg,{'game':str(path),'game_hash':a.digest(path.read_bytes())},2,self.db)
        self.assertEqual(result['agent'],teacher)
        self.assertEqual(result['original_agent'],g['white'])
        self.assertEqual(result['args'][-2:],['64','10000'])
        self.assertEqual([r['completed_depth'] for r in result['iterations']],[1,2])
        db=a.connect(self.run,a.catalogue(cfg))
        self.assertEqual(db.execute('select count(*) from searches').fetchone()[0],0)
        a.atomic(self.run/'state.json',{'slots':[{'id':1,'status':'done','game_path':str(path)}]})
        a.scan(db,self.run,labels=True);a.scan(db,self.run,labels=True)
        self.assertEqual(db.execute('select count(*) from files').fetchone()[0],1)
        db.close()
