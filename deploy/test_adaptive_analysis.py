import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
import adaptive_analysis as pool

class PoolTests(unittest.TestCase):
    def test_pool_claims_unique_positions_and_releases_cpus_after_drain(self):
        with tempfile.TemporaryDirectory() as tmp:
            run = Path(tmp); control = run/'analysis'; (control/'moments').mkdir(parents=True)
            config = dict(run=str(run), cpus=list(range(8)), adaptive_pool=True)
            pool.a.atomic(control/'dual-label-config.json', config)
            pool.a.atomic(control/'supervisor.json', dict(state='running'))
            pool.a.atomic(run/'state.json', dict(slots=[]))
            db = pool.a.connect(run, 'training-labels.sqlite')
            for i in range(9): db.execute("INSERT INTO moments VALUES(?,0,'pending',?,NULL)", (str(i),json.dumps(dict(id=str(i)))))
            db.commit()
            launched=[]; peak=[0]; processes=[]
            class Job:
                def __init__(self, target, args):
                    self.row_id=args[2]; self.ticks=0; self.exitcode=0; self.started=False
                def start(self):
                    self.started=True; launched.append(self.row_id); processes.append(self)
                def is_alive(self):
                    self.ticks+=1
                    if self.ticks<=3: return True
                    db.execute('INSERT OR IGNORE INTO dual_paired VALUES(?)',(self.row_id,));db.commit()
                    return False
                def join(self, **kwargs): pass
                def terminate(self): pass
            ticks=[0]
            def tick(_):
                ticks[0]+=1
                self.assertLess(ticks[0],100)
                with pool.allocation(control) as d:
                    for cpu in [1,3,4,5,6,7]:
                        if cpu not in d['assigned'] and len(d['assigned'])<min(6,d['pending']):d['assigned'].append(cpu)
                    peak[0]=max(peak[0],len(d['assigned']))
                    if d['pending']==0 and not d['assigned']:pool.a.STOPPING=True
            pool.a.STOPPING=False
            try:
                with patch.object(pool.os,'sched_getaffinity',return_value=set(range(8))), patch.object(pool.a,'validate_source'), patch.object(pool.dual,'teacher_config',return_value={}), patch.object(pool.a,'alive',return_value=True), patch.object(pool.champion_teacher,'check',side_effect=lambda a,r,c:c), patch.object(pool.teacher_refresh,'apply_backfill'), patch.object(pool.winner_teacher,'scan'), patch.object(pool.a,'scan'), patch.object(pool.multiprocessing,'get_context',return_value=SimpleNamespace(Process=Job)), patch.object(pool.time,'sleep',side_effect=tick):
                    pool.run(config)
                self.assertEqual(sorted(launched), [str(i) for i in range(9)])
                self.assertEqual(peak[0],6)
                demand=pool.a.read(control/'analysis-demand.json')
                self.assertEqual(demand['assigned'],[])
                self.assertEqual(demand['pending'],0)
            finally:
                pool.a.STOPPING=False; db.close()
