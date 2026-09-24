import tempfile
import unittest
from pathlib import Path

import numpy as np
import torch

from pilot_data import position_key, stratified
from pilot_fit import calibration, from_quantized, losses, weights_for
from quantized import Quantized
from train import Net, export


class PilotTests(unittest.TestCase):
    def test_outcome_mixture_and_missing_draw_target(self):
        pred=torch.tensor([0.,0.,0.],requires_grad=True)
        target=torch.zeros(3); material=torch.zeros(3)
        loss=losses(pred,target,material,torch.tensor([1.,0.,float('nan')]),'wdl',2000,.1)[0]
        loss.sum().backward()
        self.assertLess(pred.grad[0],0)
        self.assertGreater(pred.grad[1],0)
        self.assertEqual(pred.grad[2],0)
        self.assertTrue(torch.isfinite(loss).all())

    def test_score_only_zero_and_huber_compatibility(self):
        pred=torch.tensor([1.,-2.]); target=torch.tensor([1.,-2.])
        args=(pred,target,torch.tensor([1200.,-300.]),torch.tensor([0.,1.]))
        self.assertEqual(float(losses(*args,'wdl',2000,0)[0].sum()),0)
        self.assertTrue(torch.equal(losses(*args,'huber',2000,0)[0],
                                   torch.nn.functional.smooth_l1_loss(pred,target,reduction='none')))

    def test_calibration_does_not_read_held_out_outcomes(self):
        rows=[dict(split='train',game=str(i),score=1000 if i<50 else -1000,
                   outcome=float(i%5!=0 if i<50 else i%5==0)) for i in range(100)]
        before=calibration(rows)
        rows += [dict(split='test',game='heldout',score=1e8,outcome=0)]*100
        self.assertEqual(before,calibration(rows))

    def test_quantized_initialization_preserves_parent(self):
        torch.manual_seed(41); torch.set_num_threads(1)
        net=Net(256,32)
        with torch.no_grad(): net.output.weight.fill_(.3)
        with tempfile.TemporaryDirectory() as d:
            path=Path(d)/'net.bin'
            export(net,path,dict(channels=92,hash='00'*32))
            restored=from_quantized(path,256); q=Quantized(path,256)
            for shift in range(4):
                us=np.arange(shift,shift+20); them=np.arange(50,80)
                with torch.no_grad():
                    result=restored(torch.tensor(np.concatenate([us,them])),torch.tensor([0,20,50])).item()*1000
                self.assertLess(abs(result-q.residual(us,them)),20)

    def test_position_dedup_ignores_traversal_order_but_not_perspective(self):
        sample=dict(offset=0,us=2,them=2)
        a=position_key(np.array([1,2,4,3],dtype='<u4'),sample)
        self.assertEqual(a,position_key(np.array([2,1,3,4],dtype='<u4'),sample))
        self.assertNotEqual(a,position_key(np.array([3,4,1,2],dtype='<u4'),sample))

    def test_sampler_is_spaced_and_reproducible(self):
        game=dict(moves=[dict(color='Black' if i%2==0 else 'White',eval=0) for i in range(500)])
        plies=stratified(game,64,'seed')
        self.assertEqual(plies,stratified(game,64,'seed'))
        self.assertTrue(all(abs(a-b)>=4 for a in plies for b in plies if a!=b))
        self.assertTrue(all(i>=16 for i in plies))

    def test_bucket_weights(self):
        samples=[dict(label_source='saved',teacher='BASE',selection='representative')]*10
        samples += [dict(label_source='saved',teacher='NNUE512',selection='representative')]*20
        samples += [dict(label_source='analysis',teacher='hash',selection='evaluation_swing')]*5
        weights=weights_for(samples,range(35))
        self.assertAlmostEqual(sum(weights.values()),35)
        self.assertAlmostEqual(sum(weights[i] for i in range(10))/35,.2)


if __name__=='__main__':
    unittest.main()
