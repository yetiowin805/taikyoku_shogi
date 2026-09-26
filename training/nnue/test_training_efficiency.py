"""Compare optimized bookkeeping against the original CPU arithmetic."""
import copy
from collections import Counter
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import numpy as np
import torch

from pilot_fit import (PilotNet, PreparedSamples, backward_batch, evaluate,
                       losses, training_loss, weights_for)
from train import batch, export


def legacy_backward(net, samples, data, ids, microbatch, mode, scale, mix):
    total, touched = 0., []
    for start in range(0, len(ids), microbatch):
        part = ids[start:start+microbatch]
        x, off, target = batch(samples, data, part)
        touched.append(x)
        material = torch.tensor([samples[i]['material'] for i in part])
        outcome = torch.tensor([float('nan') if samples[i]['outcome'] is None
                                else samples[i]['outcome'] for i in part])
        loss = losses(net(x, off), target, material, outcome, mode, scale, mix)[0].sum()/len(ids)
        loss.backward()
        net.embedding.weight.grad = net.embedding.weight.grad.coalesce()
        total += loss.item()*len(ids)
    return total, torch.unique(torch.cat(touched))


class EfficiencyTests(unittest.TestCase):
    def setUp(self):
        torch.set_num_threads(1)
        torch.manual_seed(20260925)
        self.data = np.array([4, 1, 4, 9, 2, 3, 7, 7, 1, 8], dtype=np.uint32)
        self.samples = [dict(offset=a, us=u, them=t, score=s, material=m, outcome=o,
                             selection='representative', teacher='NNUE',
                             label_source=label, source=source)
                        for a,u,t,s,m,o,label,source in [
                            (0, 3, 1, 500, 20, None, 'saved', 'historical'),
                            (4, 0, 2, -300000, 20.5, 0., 'saved', 'recent'),
                            (6, 2, 2, 300000, -100, 1., 'analysis', 'recent'),
                            (10, 0, 0, 20, 20, .5, 'saved', 'recent')]]
        self.prepared = PreparedSamples(self.samples, self.data)

    def test_packing_preserves_repeats_empty_bags_and_clipped_targets(self):
        self.assertIs(self.prepared.data, self.data)
        for ids in ([2, 0, 2, 1, 3], [0], [1], [3]):
            actual = self.prepared.batch(ids)
            for a, b in zip(actual[:3], batch(self.samples, self.data, ids)):
                self.assertTrue(torch.equal(a, b))
            expected_material = torch.tensor([self.samples[i]['material'] for i in ids])
            torch.testing.assert_close(actual[3], expected_material.to(actual[3].dtype), rtol=0, atol=0)

    def test_objective_and_gradient_match_full_diagnostics(self):
        _, _, target, material, outcome = self.prepared.batch([0, 1, 2, 3])
        for mode, mix in [('huber', 0), ('wdl', 0), ('wdl', .1), ('wdl', 1)]:
            pred = torch.tensor([.2, -4., 8., 0.], requires_grad=True)
            old = losses(pred, target, material, outcome, mode, 2000, mix)[0]
            new = training_loss(pred, target, material, outcome, mode, 2000, mix)
            self.assertTrue(torch.equal(old, new))
            a, = torch.autograd.grad(old.sum(), pred)
            b, = torch.autograd.grad(new.sum(), pred)
            self.assertTrue(torch.equal(a, b))

    def test_updates_and_touched_rows_match_with_and_without_prior_gradients(self):
        for mode, mix in [('huber', 0), ('wdl', 0), ('wdl', .1)]:
            for microbatch in (1, 2, 4):
                a = PilotNet(16, 32)
                b = copy.deepcopy(a)
                # Two calls without zeroing also exercise the public accumulation path.
                for ids in ([2, 0, 2, 1, 3], [3], [1]):
                    old, old_rows = legacy_backward(a, self.samples, self.data, ids,
                                                   microbatch, mode, 2000, mix)
                    new, new_rows = backward_batch(b, self.samples, self.data, ids,
                                                  microbatch, mode, 2000, mix, self.prepared)
                    self.assertEqual(old, new)
                    self.assertTrue(torch.equal(old_rows, new_rows))
                    for x, y in zip(a.parameters(), b.parameters()):
                        gx = x.grad.to_dense() if x.grad.is_sparse else x.grad
                        gy = y.grad.to_dense() if y.grad.is_sparse else y.grad
                        self.assertTrue(torch.equal(gx, gy))
                for net in (a, b):
                    torch.optim.SGD([net.embedding.weight], lr=.0002).step()
                    torch.optim.Adam([v for k,v in net.named_parameters()
                                      if k != 'embedding.weight'], lr=.0003).step()
                for x,y in zip(a.parameters(), b.parameters()):
                    self.assertTrue(torch.equal(x,y))

    def test_validation_preserves_source_metrics_and_summation_order(self):
        net = PilotNet(16, 32)
        ids = [2, 0, 1, 0, 3]
        weights = weights_for(self.samples, ids)
        totals, sources = Counter(), {}
        with torch.no_grad():
            for start in range(0, len(ids), 2):
                group = ids[start:start+2]
                x, off, target = batch(self.samples, self.data, group)
                material = torch.tensor([self.samples[i]['material'] for i in group])
                outcome = torch.tensor([float('nan') if self.samples[i]['outcome'] is None
                                        else self.samples[i]['outcome'] for i in group])
                pred = net(x, off)
                loss, huber, p, q, mask = losses(pred, target, material, outcome, 'wdl', 2000, .1)
                w = torch.tensor([weights[i] for i in group])
                for name, value in [('objective', (loss*w).sum()), ('huber', huber.sum()),
                                    ('mae', (pred-target).abs().sum()*1000),
                                    ('teacher_brier', (p-q).square().sum()),
                                    ('outcome_brier', (p[mask]-outcome[mask]).square().sum()),
                                    ('outcome_count', mask.sum())]:
                    # Original MAE multiplies the Python float, not the tensor.
                    totals[name] += ((pred-target).abs().sum().item()*1000
                                     if name == 'mae' else value.item())
                for j,i in enumerate(group):
                    s = self.samples[i]
                    key = 'analysis' if s['label_source'] == 'analysis' else s['source']
                    c = sources.setdefault(key, Counter())
                    c['count'] += 1
                    c['huber'] += huber[j].item()
        expected = {k:v/(totals['outcome_count'] if k == 'outcome_brier' else len(ids))
                    for k,v in totals.items() if k != 'outcome_count'}
        expected.update(count=len(ids), outcome_count=totals['outcome_count'],
                        by_source={k:dict(count=v['count'], huber=v['huber']/v['count'])
                                   for k,v in sources.items()})
        self.assertEqual(evaluate(net, self.samples, self.data, ids, 2, 'wdl', 2000, .1), expected)

    def test_cli_checkpoint_resume_matches_uninterrupted_training(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            schema = dict(features=16, channels=92, hash='00'*32, names_hash='11'*32)
            (root/'schema.json').write_text(json.dumps(schema))
            np.tile(np.array([0, 1, 2, 3], dtype='<u4'), 60).tofile(root/'features.bin')
            samples = [dict(offset=i*4, us=2, them=2, score=1000, material=100,
                            outcome=float(i%4 != 0), group=str(i), game=str(i),
                            split='train' if i<40 else 'validation' if i<50 else 'test',
                            source='recent', label_source='saved', selection='representative',
                            teacher='NNUE') for i in range(60)]
            (root/'samples.jsonl').write_text(''.join(json.dumps(s)+'\n' for s in samples))
            identity = {name+'_sha256':hashlib.sha256((root/(name+'.'+suffix)).read_bytes()).hexdigest()
                        for name,suffix in [('features','bin'),('samples','jsonl'),('schema','json')]}
            (root/'dataset.json').write_text(json.dumps(identity))
            sha = export(PilotNet(16,32), root/'parent.bin', schema)
            (root/'parent.json').write_text(json.dumps(dict(weights=dict(nnue=dict(
                file='parent.bin', sha256=sha, feature_hash=schema['names_hash'], width=32)))))
            options = [str(root), '--parent', str(root/'parent.json'), '--init', 'fresh',
                       '--loss', 'wdl', '--width', '32', '--epochs', '4',
                       '--epoch-samples', '8', '--microbatch', '1', '--defer-test']
            directory = Path(__file__).parent
            subprocess.run([sys.executable, str(directory/'pilot_fit.py'), *options,
                            '--out', str(root/'full')], check=True, capture_output=True, timeout=60)
            # Exit after the first checkpoint's atomic publication, at the normal log boundary.
            runner = """import json,sys,pilot_fit as fit
def checkpoint_exit(message, **kwargs):
    if json.loads(message).get('epoch') == 1:
        raise SystemExit(99)
fit.print = checkpoint_exit
fit.main()
"""
            interrupted = subprocess.run([sys.executable, '-c', runner, *options,
                                          '--out', str(root/'resumed')], cwd=directory,
                                         capture_output=True, timeout=60)
            self.assertEqual(interrupted.returncode, 99, interrupted.stderr)
            subprocess.run([sys.executable, str(directory/'pilot_fit.py'), *options,
                            '--out', str(root/'resumed'), '--resume'],
                           check=True, capture_output=True, timeout=60)
            full, resumed = [torch.load(root/name/'training.pt', weights_only=True)
                             for name in ('full','resumed')]
            self.assertEqual(full['epoch'], 4)
            for name, value in full['net'].items():
                self.assertTrue(torch.equal(value, resumed['net'][name]))
            self.assertEqual([m['validation'] for m in full['metrics']],
                             [m['validation'] for m in resumed['metrics']])
            self.assertEqual(full['plateau'], resumed['plateau'])
            blobs = [json.loads((root/name/'model.json').read_text())['weights']['nnue']['sha256']
                     for name in ('full','resumed')]
            self.assertEqual(*blobs)


if __name__ == '__main__':
    unittest.main()
