"""Reuse the exact-route harness; report CPU time because this run is throttled."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import shutil
import subprocess
import sys
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'benchmarks/search_speed_20260920'))
import run as harness
p=argparse.ArgumentParser(description=__doc__)
for name in ['baseline','candidate','corpus','cases','out']:
    p.add_argument('--'+name,type=Path,required=True)
p.add_argument('--cpu',type=int,default=11)
p.add_argument('--repeats',type=int,default=2)
p.add_argument('--variant',required=True)
a=p.parse_args();a.out.mkdir(parents=True,exist_ok=False);(a.out/'bin').mkdir()
shutil.copyfile(a.corpus,a.out/'corpus.json')
cases=[{k:r[k] for k in ['position','model','depth']} for r in json.loads(a.cases.read_text())]
plan=dict(variant=a.variant,base='61c2875',cases=cases,repeats=a.repeats,cpu=a.cpu,clock='process CPU milliseconds',
          source_hashes={str(f.relative_to(ROOT)):hashlib.sha256(f.read_bytes()).hexdigest() for f in (ROOT/'src').rglob('*.rs')},
          compiler=subprocess.check_output(['rustc','-Vv'],text=True),binaries={})
for name in ['baseline','candidate']:
    path=getattr(a,name).resolve();(a.out/'bin'/name).symlink_to(path)
    plan['binaries'][name]=dict(path=str(path),sha256=hashlib.sha256(path.read_bytes()).hexdigest())
(a.out/'plan.json').write_text(json.dumps(plan,indent=2)+'\n')
harness.OUT=a.out;servers={};rows=[]
try:
    for name in ['baseline','candidate']: servers[name]=harness.Server(name,'paired',a.cpu)
    for case in cases: harness.compare(servers['baseline'].request(case),servers['candidate'].request(case))
    print('WARMUP',a.variant,len(cases),'exact matches',flush=True)
    rng=random.Random(20260926);first={(c['position'],c['model']):rng.randrange(2) for c in cases}
    with (a.out/'pairs.jsonl').open('w') as f:
        for rep in range(a.repeats):
            order=cases.copy();rng.shuffle(order)
            for case in order:
                names=['baseline','candidate']
                if (rep+first[(case['position'],case['model'])])%2:names.reverse()
                results={n:servers[n].request(case) for n in names}
                harness.compare(results['baseline'],results['candidate'])
                row=dict(rep=rep,**case,order=names,measurements=results)
                f.write(json.dumps(row)+'\n');f.flush();rows.append(row)
            print('MEASURED',a.variant,rep+1,flush=True)
finally:
    for server in servers.values():server.close()
def ratio(group):
    return math.exp(sum(math.log(r['measurements']['candidate']['cpu_ms']/r['measurements']['baseline']['cpu_ms']) for r in group)/len(group))
summary=dict(variant=a.variant,pairs=len(rows),candidate_over_baseline_cpu=ratio(rows),
             by_model={m:ratio([r for r in rows if r['model']==m]) for m in sorted({r['model'] for r in rows})},
             by_position={i:ratio([r for r in rows if r['position']==i]) for i in sorted({r['position'] for r in rows})},
             total_cpu_ms={n:sum(r['measurements'][n]['cpu_ms'] for r in rows) for n in ['baseline','candidate']},
             peak_rss_kib={n:max(r['measurements'][n]['peak_rss_kib'] for r in rows) for n in ['baseline','candidate']},
             caveat='CPU-time smoke screen under duty-cycling and concurrent user workload; no normal-load wall speedup or strength claim.')
(a.out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary),flush=True)
