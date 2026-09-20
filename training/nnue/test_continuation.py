import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from dataclasses import asdict
import numpy as np
import torch
from plateau import Plateau, Policy
from train import Net, restore_net

HERE = Path(__file__).resolve().parent


class PolicyTests(unittest.TestCase):
    def test_minimum_halvings_plateau_and_resume(self):
        policy = Policy(min_epochs=8, max_epochs=32, patience=3)
        tracker = Plateau(1.)
        for epoch in range(3, 8):
            self.assertEqual(tracker.observe(epoch, 1., policy), 'continue')
        self.assertEqual(tracker.observe(8, 1., policy), 'reduce_lr')
        snapshot = tracker.record(policy)
        resumed = Plateau(**snapshot['state'])
        for epoch in range(9, 15):
            self.assertEqual(tracker.observe(epoch, 1., policy), resumed.observe(epoch, 1., policy))
        self.assertEqual(tracker.stopped, 'plateau')
        self.assertEqual(tracker.reductions, 2)

    def test_small_gains_accumulate_and_cap_is_not_plateau(self):
        p = Policy(min_epochs=3, max_epochs=4)
        t = Plateau(1.)
        t.observe(1, .997, p)
        t.observe(2, .994, p)
        self.assertEqual(t.bad_epochs, 0)
        self.assertAlmostEqual(t.best, .994)
        self.assertEqual(t.observe(4, .8, p), 'max_epochs')
        with self.assertRaises(ValueError):
            Plateau(1.).observe(1, float('nan'), p)
        with self.assertRaises(ValueError):
            Policy(max_epochs=32.5)


class ResumeTests(unittest.TestCase):
    def test_mmap_assignment_and_optimizer_restore_leave_seed_immutable(self):
        torch.set_num_threads(1)
        net = Net(256, 32)
        opt = torch.optim.Adam([net.h1.weight], lr=.001)
        x, offsets = torch.tensor([1, 2, 3, 4]), torch.tensor([0, 2, 4])
        net(x, offsets).sum().backward(); opt.step(); opt.zero_grad(set_to_none=True)
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / 'state.pt'
            torch.save({'net': net.state_dict(), 'optimizer': opt.state_dict()}, path)
            before = hashlib.sha256(path.read_bytes()).hexdigest()
            cp = torch.load(path, mmap=True, weights_only=True)
            loaded = restore_net(cp, 256, 32)
            self.assertEqual(loaded.embedding.weight.data_ptr(), cp['net']['embedding.weight'].data_ptr())
            second = torch.optim.Adam([loaded.h1.weight], lr=.001)
            second.load_state_dict(cp['optimizer'])
            for model, optimizer in [(net, opt), (loaded, second)]:
                model(x, offsets).sum().backward(); optimizer.step()
            torch.testing.assert_close(net.h1.weight, loaded.h1.weight)
            self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), before)

    def test_tiny_training_fork_plateau_and_restart(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d); dataset = root / 'dataset'; dataset.mkdir()
            seed = root / 'seed'; out = root / 'continuation'
            base = root / 'base.json'; base.write_text(json.dumps({'weights': {'piece': {}}, 'search_defaults': {}}))
            samples = []; values = []
            for i in range(32):
                samples.append(dict(offset=len(values), us=3, them=3, score=300 + i, material=0,
                                    split='train' if i < 24 else 'validation', game=str(i)))
                values.extend([(i + j) % 256 for j in range(6)])
            (dataset / 'features.bin').write_bytes(np.array(values, dtype='<u4').tobytes())
            (dataset / 'samples.jsonl').write_text(''.join(json.dumps(x)+'\n' for x in samples))
            (dataset / 'schema.json').write_text(json.dumps(dict(features=256, channels=92, hash='00'*32, names_hash='00'*32)))
            def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
            meta = dict(version=1, samples=32, baseline_sha256=sha(base))
            for name, suffix in [('features', 'bin'), ('samples', 'jsonl'), ('schema', 'json')]:
                meta[name+'_sha256'] = sha(dataset / f'{name}.{suffix}')
            (dataset / 'dataset.json').write_text(json.dumps(meta))
            def command(*args):
                return subprocess.run([sys.executable, *map(str,args)], check=True, capture_output=True, text=True, timeout=60)
            command(HERE/'train.py',dataset,'--base',base,'--out',seed,'--width',32,'--threads',1,'--epochs',2)
            original = {p.name: sha(p) for p in seed.iterdir() if p.suffix in ('.pt','.bin','.json')}
            # Stop during epoch 4 validation, then compare the resumed result with
            # uninterrupted training, including sparse updates and Adam state.
            interrupted = root / 'interrupted'; clean = root / 'clean'
            options = [dataset,'--base',base,'--width',32,'--threads',1,'--epochs',4,'--generation','v2',
                       '--plateau','--min-epochs',3,'--patience',10]
            fork = ['--resume-from',seed/'w32.training.pt','--seed-model',seed/'NNUE_W32_v1.json']
            wrapper = (f'import sys;sys.path.insert(0,{str(HERE)!r});import train\n'
                       'validate=train.validate;calls=0\n'
                       'def interrupt(*args):\n'
                       ' global calls\n calls+=1\n'
                       ' if calls==2:raise InterruptedError("fixture interruption")\n'
                       ' return validate(*args)\n'
                       'train.validate=interrupt\n'
                       'try:train.main()\nexcept InterruptedError:sys.exit(130)\n')
            with self.assertRaises(subprocess.CalledProcessError) as stopped:
                command('-c',wrapper,*options,'--out',interrupted,*fork)
            self.assertEqual(stopped.exception.returncode,130)
            saved = torch.load(interrupted/'w32.training.pt',weights_only=True,mmap=True)
            self.assertEqual(saved['epoch'],3)
            command(HERE/'train.py',*options,'--out',interrupted,'--resume')
            command(HERE/'train.py',*options,'--out',clean,*fork)
            restored = torch.load(interrupted/'w32.training.pt',weights_only=True,mmap=True)
            expected = torch.load(clean/'w32.training.pt',weights_only=True,mmap=True)
            for name, tensor in expected['net'].items():
                torch.testing.assert_close(restored['net'][name],tensor,rtol=0,atol=0)
            self.assertEqual(restored['plateau'],expected['plateau'])
            cfg = root / 'job.json'
            command(HERE/'continuation.py','prepare','--dataset',dataset,'--base',base,'--checkpoints',seed,
                    '--seed-models',seed,'--out',out,'--python',sys.executable,'--config',cfg,'--widths',32)
            config = json.loads(cfg.read_text())
            config['policy'] = asdict(Policy(min_epochs=3,max_epochs=4,patience=1,min_improvement=.99,lr_reductions=0))
            cfg.write_text(json.dumps(config))
            command(HERE/'continuation.py','check',cfg)
            sys.path.insert(0,str(HERE.parent.parent/'deploy'))
            import tourney_analysis as coordinator
            pinned = coordinator.training_sidecar.prepare(coordinator,str(cfg),root/'run')
            command(*pinned['command'][1:])
            progress = json.loads((out/'progress.json').read_text())
            self.assertEqual(progress['state'],'completed')
            self.assertEqual(progress['widths']['32']['reason'],'plateau')
            checkpoint = sha(out/'w32.training.pt')
            resumed = coordinator.training_sidecar.prepare(coordinator,None,root/'run',previous=pinned)
            self.assertEqual(resumed,pinned)
            command(*resumed['command'][1:])
            self.assertEqual(sha(out/'w32.training.pt'),checkpoint)
            self.assertEqual(original,{p.name:sha(p) for p in seed.iterdir() if p.suffix in ('.pt','.bin','.json')})
            self.assertEqual(len(list(out.glob('nnue-*.bin'))),1)
            config['policy']['patience'] = 2;cfg.write_text(json.dumps(config))
            with self.assertRaises(subprocess.CalledProcessError):command(HERE/'continuation.py','check',cfg)


if __name__ == '__main__':unittest.main()
