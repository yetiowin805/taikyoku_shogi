import copy
import json
import subprocess
from unittest.mock import patch

from test_tourney_analysis import AnalyzerFixture, a
import champion_teacher as c


class ChampionTests(AnalyzerFixture):
    def fixture(self, lead=50):
        model=self.run/'challenger.json';a.atomic(model,dict(name='challenger',weights={}))
        binary=self.run/'rater';binary.write_text('test')
        config=dict(new_sha256='current-sha',new_teacher_id='current',old=dict(models={str(model):dict(sha256=a.file_digest(model))}),
                    auto_champion=dict(ratings_bin=str(binary),ratings_sha256=a.file_digest(binary),margin=50))
        a.atomic(self.run/'analysis/dual-label-config.json',config)
        a.atomic(self.run/'state.json',dict(entrants=[dict(id='current',model='current.json'),dict(id='challenger',model=str(model))],slots=[dict(id=1,model_a='current',model_b='challenger',score_a=0,status='done')]))
        fit=dict(status='converged',games=1,ratings=dict(current=1500,challenger=1500+lead))
        return config,fit

    def execute(self, config, fit):
        with patch.object(a.subprocess,'run',return_value=subprocess.CompletedProcess([],0,json.dumps(fit),'')):
            return c.check(a,self.run,config)

    def test_tie_holds_but_any_positive_lead_switches(self):
        config,fit=self.fixture(0)
        with patch.object(c.teacher_refresh,'replace') as replace:
            self.execute(config,fit);replace.assert_not_called()
        # Same results/policy/incumbent must not rerun the solver.
        with patch.object(a.subprocess,'run',side_effect=AssertionError('repeated fit')):
            c.check(a,self.run,config)
        self.assertEqual(a.read(self.run/'analysis/champion-status.json')['state'],'holding')
        (self.run/'analysis/champion-status.json').unlink()
        fit['ratings']['challenger']=1500.01
        def switch(*args,**kwargs):
            updated=copy.deepcopy(config);updated.update(new_sha256='new',new_teacher_id='challenger')
            a.atomic(self.run/'analysis/dual-label-config.json',updated)
            return dict(changed=True,teacher='challenger',backfill_positions=8)
        with patch.object(c.teacher_refresh,'replace',side_effect=switch) as replace:
            result=self.execute(config,fit)
        self.assertEqual(result['new_teacher_id'],'challenger');self.assertEqual(replace.call_count,1)
        self.assertAlmostEqual(a.read(self.run/'analysis/champion-status.json')['lead'],0.01)

    def test_no_fit_or_mutated_model_keeps_teacher(self):
        config,fit=self.fixture(100)
        fit['status']='not_converged'
        with patch.object(c.teacher_refresh,'replace') as replace:
            self.execute(config,fit);replace.assert_not_called()
        self.assertEqual(a.read(self.run/'analysis/champion-status.json')['state'],'no_finite_fit')
        (self.run/'analysis/champion-status.json').unlink()
        fit['status']='converged';(self.run/'challenger.json').write_text('{}')
        self.execute(config,fit)
        self.assertEqual(a.read(self.run/'analysis/champion-status.json')['state'],'error')
        self.assertEqual(a.read(self.run/'analysis/dual-label-config.json')['new_teacher_id'],'current')

    def test_provisional_leader_is_selected_and_retired_leader_excluded(self):
        config,fit=self.fixture(100)
        fit.update(status='separated',components=[['current'],['challenger']])
        with patch.object(c.teacher_refresh,'replace',return_value={}) as replace:
            self.execute(config,fit);replace.assert_called_once()
        state=a.read(self.run/'state.json');state['retired']=['challenger']
        a.atomic(self.run/'state.json',state)
        with patch.object(c.teacher_refresh,'replace') as replace:
            self.execute(config,fit);replace.assert_not_called()
        self.assertEqual(a.read(self.run/'analysis/champion-status.json')['leader'],'current')

    def test_error_after_durable_switch_reloads_config(self):
        config,fit=self.fixture(100)
        def interrupted(*args,**kwargs):
            newer=dict(config,new_teacher_id='challenger')
            a.atomic(self.run/'analysis/dual-label-config.json',newer)
            raise RuntimeError('simulated post-commit interruption')
        with patch.object(c.teacher_refresh,'replace',side_effect=interrupted):
            result=self.execute(config,fit)
        self.assertEqual(result['new_teacher_id'],'challenger')
