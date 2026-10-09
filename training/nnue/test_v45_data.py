import unittest
from collections import defaultdict
from v45_data import reweight, multiplier

class WeightingTests(unittest.TestCase):
    def test_no_epoch_limit_still_stops_on_plateau(self):
        from plateau import Policy, Plateau
        policy=Policy(min_epochs=1,max_epochs=None,patience=2,lr_reductions=0)
        tracker=Plateau(10)
        self.assertEqual(tracker.observe(1000000,9,policy),"continue")
        self.assertEqual(tracker.observe(1000001,9,policy),"continue")
        self.assertEqual(tracker.observe(1000002,9,policy),"plateau")

    def test_boundaries(self):
        self.assertEqual([multiplier(x) for x in (0,.099,.1,.25,.251,1)], [1,1,1.5,1.5,2,2])

    def test_strata_and_game_mass_preserved_without_holdout_boost(self):
        samples=[]
        for split in ('train','validation','test'):
            for stratum, mass in [('representative',.75),('precursor',.2),('mate',.05)]:
                for game in ('a','b'):
                    for difficulty in (1.,2.):
                        samples.append(dict(split=split,stratum=stratum,group=game,game=game,episode='x',
                            sampling_weight=mass/4,difficulty_multiplier=difficulty,score=123))
        reweight(samples)
        totals=defaultdict(float)
        for s in samples:
            totals[s['split'],s['stratum'],s['game']]+=s['sampling_weight']
            self.assertEqual(s['score'],123)
        for split in ('train','validation','test'):
            for k,mass in [('representative',.9),('precursor',.08),('mate',.02)]:
                self.assertAlmostEqual(totals[split,k,'a'],mass/2)
                self.assertAlmostEqual(totals[split,k,'b'],mass/2)
        self.assertAlmostEqual(samples[1]['sampling_weight']/samples[0]['sampling_weight'],2)
        self.assertAlmostEqual(samples[13]['sampling_weight']/samples[12]['sampling_weight'],1)

class PreparationTests(unittest.TestCase):
    def test_actual_export_prepare_resume_and_heldout_weights(self):
        import json, subprocess, sys, tempfile
        from pathlib import Path
        import numpy as np
        import torch
        from train import Net, export
        from pilot_data import digest
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); source=root/'source'; source.mkdir()
            schema=dict(features=16,channels=92,hash='00'*32,names_hash='11'*32)
            (source/'schema.json').write_text(json.dumps(schema))
            sha=export(Net(16,32),root/'parent.bin',schema)
            (root/'parent.json').write_text(json.dumps(dict(weights=dict(nnue=dict(
                file='parent.bin',sha256=sha,feature_hash=schema['names_hash'],width=32)))))
            rows=[]
            for split in ('train','validation','test'):
                for k,weight in [('representative',.75),('precursor',.20),('mate',.05)]:
                    rows.append(dict(split=split,stratum=k,group=split,game=split,episode='phase',
                        sampling_weight=weight,offset=len(rows)*2,us=1,them=1,score=1000.,
                        material=100.,target_probability=.6,outcome=1.))
            (source/'samples.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in rows))
            np.arange(18,dtype='<u4').__mod__(16).tofile(source/'features.bin')
            (source/'dataset.json').write_text(json.dumps(dict(scale=2822.9057434005167,
                samples_sha256=digest(source/'samples.jsonl'),schema_sha256=digest(source/'schema.json'),
                feature_shards=[dict(path=str(source/'features.bin'),sha256=digest(source/'features.bin'))])))
            command=[sys.executable,str(Path(__file__).with_name('v45_data.py')),'--source',str(source),
                     '--parent',str(root/'parent.json'),'--out',str(root/'out')]
            subprocess.run(command,check=True,capture_output=True)
            result=[json.loads(x) for x in (root/'out/samples.jsonl').read_text().splitlines()]
            for before,after in zip(rows,result):
                self.assertEqual(before['score'],after['score'])
                self.assertEqual(before['target_probability'],after['target_probability'])
                self.assertAlmostEqual(after['sampling_weight'],dict(representative=.9,precursor=.08,mate=.02)[after['stratum']])
                self.assertEqual('difficulty_multiplier' in after,after['split']=='train')
            first=digest(root/'out/samples.jsonl')
            subprocess.run(command,check=True,capture_output=True)
            self.assertEqual(first,digest(root/'out/samples.jsonl'))

if __name__=='__main__':unittest.main()
