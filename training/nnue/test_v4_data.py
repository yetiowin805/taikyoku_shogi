import copy,json,tempfile,unittest
from collections import defaultdict
from pathlib import Path
import numpy as np
from pilot_data import digest
from v4_data import combine,Features,set_weights,bucket,probability

class V4DataTests(unittest.TestCase):
    def test_teacher_probability_mean_and_completed_mates(self):
        rows=[dict(model_sha256='a',score=1000,completed_depth=4),dict(model_sha256='b',score=-1000,completed_depth=3),dict(model_sha256='bad',score=999999,completed_depth=0)]
        self.assertAlmostEqual(combine(rows)['score'],0)
        self.assertFalse(combine(rows)['mate'])
        rows.append(dict(model_sha256='a',score=900010,completed_depth=5))
        value=combine(rows)
        self.assertTrue(value['mate']);self.assertEqual(len(value['teachers']),2)
        self.assertAlmostEqual(probability(value['score']),(1+probability(-1000))/2)
        self.assertIsNone(combine([dict(score=900000,completed_depth=0)]))
    def test_weighting_caps_targeted_and_long_games(self):
        rows=[]
        for kind,source,selection,mate,n in [('old','historical','representative',False,7),('new','recent','representative',False,20),('tactic','recent','evaluation_swing',False,30),('mate','recent','decisive_ending',True,4)]:
            for game,count in [('a',n),('b',1)]:
                rows.extend(dict(split='train',game_sha256=kind+game,source=source,selection=selection,mate=mate) for _ in range(count))
        set_weights(rows);totals=defaultdict(float);games=defaultdict(float)
        for r in rows:totals[bucket(r)]+=r['sampling_weight'];games[r['game_sha256']]+=r['sampling_weight']
        for k,v in dict(historical=.2,recent=.55,targeted=.2,mate=.05).items():self.assertAlmostEqual(totals[k],v)
        self.assertAlmostEqual(games['newa'],games['newb'])
    def test_feature_shards_are_verified_and_never_joined_or_written(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);shards=[]
            for name,values in [('old',[1,2,3]),('new',[4,5])]:
                p=root/name;np.array(values,dtype='<u4').tofile(p);shards.append(dict(path=str(p),sha256=digest(p)))
            (root/'dataset.json').write_text(json.dumps(dict(feature_shards=shards)))
            f=Features(root);self.assertEqual(f[3:5].tolist(),[4,5]);self.assertEqual(f[0:3].tolist(),[1,2,3])
            with self.assertRaises(IndexError):f[2:4]
            (root/'old').write_bytes(b'changed')
            with self.assertRaises(ValueError):Features(root)

if __name__=='__main__':unittest.main()
