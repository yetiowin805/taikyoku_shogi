import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import numpy as np
import torch
import generation
from pilot_fit import PilotNet,backward_batch


class GenerationTests(unittest.TestCase):
    def test_microbatch_preserves_effective_gradient(self):
        torch.set_num_threads(1);torch.manual_seed(17)
        a=PilotNet(256,32);b=copy.deepcopy(a)
        data=np.array([1,2,3,8,9,10,2,3,4,8,10,11],dtype=np.uint32)
        samples=[dict(offset=i*6,us=3,them=3,score=500+i*100,material=20,outcome=None) for i in range(2)]
        la,_=backward_batch(a,samples,data,[0,1],2,'wdl',2000,0)
        lb,_=backward_batch(b,samples,data,[0,1],1,'wdl',2000,0)
        self.assertAlmostEqual(la,lb,places=6)
        for x,y in zip(a.parameters(),b.parameters()):
            gx=x.grad.to_dense() if x.grad.is_sparse else x.grad
            gy=y.grad.to_dense() if y.grad.is_sparse else y.grad
            self.assertTrue(torch.allclose(gx,gy,atol=1e-6,rtol=1e-4))

    def test_ranking_is_order_and_perspective_invariant(self):
        s=dict(entrants=[dict(id=n) for n in 'ABC'],slots=[dict(status='done',model_a=a,model_b=b,score_a=v) for a,b,v in [('A','B',1),('B','C',1),('A','C',.5)]])
        rank=generation.standings(s)
        t=copy.deepcopy(s);t['slots'].reverse()
        for g in t['slots']:g['model_a'],g['model_b']=g['model_b'],g['model_a'];g['score_a']=1-g['score_a']
        other=generation.standings(t)
        self.assertEqual([r['id'] for r in rank],['C','B','A'])
        self.assertTrue(np.allclose([r['rating'] for r in rank],[r['rating'] for r in other]))

    def rollout_fixture(self,root):
        repo=root/'repo';old=repo/'data/raw/old';out=root/'output'
        (old/'analysis').mkdir(parents=True);out.mkdir();(repo/'target/release').mkdir(parents=True)
        binary=repo/'target/release/taikyoku_shogi';binary.write_bytes(b'engine')
        (repo/'data/run').mkdir();(repo/'data/run/royal-nnue-current-run.txt').write_text('data/raw/old')
        entrants=[]
        for i in range(32):
            name='NNUE_W512_v2' if i==31 else f'old{i:02d}'
            model=repo/(name+'.json');model.write_text('{}');entrants.append(dict(id=name,model=str(model)))
        slots=[dict(status='done',model_a=entrants[i]['id'],model_b=entrants[j]['id'],score_a=0) for i in range(32) for j in range(i+1,32)]
        generation.write(old/'state.json',dict(entrants=entrants,slots=slots))
        generation.write(old/'analysis/config.json',dict(command=[str(binary)]))
        for p in ['manifest.json','supervisor.json']:generation.write(old/'analysis'/p,{})
        models=[]
        for w in generation.WIDTHS:
            model=out/f'NNUE_W{w}_v3.json';blob=out/f'{w}.bin';blob.write_bytes(b'weights')
            generation.write(model,dict(name=f'NNUE_W{w}_v3',weights=dict(nnue=dict(file=blob.name))))
            models.append(model)
        return dict(repo=str(repo),old_run=str(old),out=str(out),new_run=str(repo/'data/raw/new'),unit='test-generation'),models

    def test_rollout_preserves_old_record_and_admits_eight(self):
        with tempfile.TemporaryDirectory() as d:
            c,models=self.rollout_fixture(Path(d));state=Path(c['old_run'])/'state.json';before=state.read_bytes()
            with patch('generation.subprocess.run'),patch('generation.external_service') as launch:
                generation.rollout(c,models)
            self.assertEqual(state.read_bytes(),before)
            field=generation.read(Path(c['out'])/'manifest.json')['entrants']
            self.assertEqual(len(field),32);self.assertEqual(sum(e['id'].endswith('_v3') for e in field),8)
            self.assertFalse(any(e['id']=='old00' for e in field));launch.assert_called_once()
            self.assertEqual(generation.read(Path(c['out'])/'rollout.json')['state'],'deployed')

    def test_launch_failure_resumes_original_run(self):
        with tempfile.TemporaryDirectory() as d:
            c,models=self.rollout_fixture(Path(d))
            with patch('generation.subprocess.run'),patch('generation.external_service',side_effect=[RuntimeError('launch failed'),None]) as launch:
                with self.assertRaisesRegex(RuntimeError,'launch failed'):generation.rollout(c,models)
            self.assertEqual(launch.call_args.args[1],'resume')
            self.assertEqual(generation.read(Path(c['out'])/'rollout.json')['state'],'rolled_back')

if __name__=='__main__':unittest.main()
