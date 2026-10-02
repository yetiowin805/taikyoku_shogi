#!/usr/bin/env python3
"""Paired behavior-changing prototypes: legal routes checked by the servers.
Run under the shared CPU lock. Fixed-time outcomes are search work, not strength.
"""
import argparse, hashlib, json, math, os, random, subprocess, time
from pathlib import Path
p=argparse.ArgumentParser()
for n in ('baseline','candidate','corpus','out'): p.add_argument('--'+n,type=Path,required=True)
p.add_argument('--models',type=int,nargs='+',default=[0,2])
p.add_argument('--positions',type=int,nargs='+',default=[0,1,2,6,7,8,18,19,20])
p.add_argument('--depth',type=int,default=3)
p.add_argument('--budget-ms',type=int,default=1500)
p.add_argument('--repeats',type=int,default=2)
p.add_argument('--cpu',type=int,default=2)
a=p.parse_args();a.out.mkdir(parents=True,exist_ok=False)
corpus=json.loads(a.corpus.read_text()); rng=random.Random(20261002)
def sha(path):
 with open(path, 'rb') as f: return hashlib.file_digest(f, 'sha256').hexdigest()
plan={k:str(v) if isinstance(v,Path) else v for k,v in vars(a).items()}
plan['binary_sha256']={n:sha(getattr(a,n)) for n in ('baseline','candidate')}
plan['corpus']=corpus
plan['metric']='fixed-depth completed searches and fixed-budget completed depth; not strength'
plan['build_flags']={'profile':'release','RUSTFLAGS':'-C target-cpu=native','cargo':'--locked --offline'}
plan['compiler']=subprocess.check_output(['rustc','-Vv'],text=True)
plan['machine']=subprocess.check_output(['lscpu'],text=True)
(a.out/'plan.json').write_text(json.dumps(plan,indent=2)+'\n')
servers={};logs={};rows=[]
try:
 for name in ('baseline','candidate'):
  logs[name]=(a.out/(name+'.stderr')).open('w')
  servers[name]=subprocess.Popen(['taskset','-c',str(a.cpu),str(getattr(a,name)),str(a.corpus)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=logs[name],text=True)
 for rep in range(a.repeats):
  for model in a.models:
   for pos in a.positions:
    q=dict(position=pos,model=model,depth=a.depth,budget_ms=a.budget_ms)
    names=list(servers)
    if (rep+pos+model)%2:names.reverse()
    measurements={}
    for name in names:
     child=servers[name];child.stdin.write(json.dumps(q)+'\n');child.stdin.flush()
     line=child.stdout.readline()
     if not line:raise RuntimeError(f'{name} exited: {child.poll()}')
     measurements[name]=json.loads(line)
    row=dict(rep=rep,**q,measurements=measurements)
    rows.append(row)
    with (a.out/'pairs.jsonl').open('a') as f:f.write(json.dumps(row)+'\n')
    print(model,pos,rep,{n:(r['completed_depth'],round(r['ms']),r['nodes']) for n,r in measurements.items()},flush=True)
finally:
 for child in servers.values():
  child.stdin.close();child.wait()
 for log in logs.values():log.close()
summary={}
for model in a.models:
 group=[r for r in rows if r['model']==model]
 same=[r for r in group if r['measurements']['baseline']['completed_depth']==r['measurements']['candidate']['completed_depth']]
 complete=[r for r in same if r['measurements']['baseline']['completed_depth']==a.depth]
 def avg_ratio(items,key):
  return math.exp(sum(math.log(r['measurements']['candidate'][key]/r['measurements']['baseline'][key]) for r in items)/len(items)) if items else None
 summary[corpus['models'][model]['name']]=dict(pairs=len(group),same_depth_pairs=len(same),complete_pairs=len(complete),
  complete_time_ratio=avg_ratio(complete,'ms'),complete_nodes_ratio=avg_ratio(complete,'nodes'),
  best_agreement=sum(r['measurements']['baseline']['best']==r['measurements']['candidate']['best'] for r in group),
  best_agreement_at_same_depth=sum(r['measurements']['baseline']['best']==r['measurements']['candidate']['best'] for r in same),
  score_agreement_at_same_depth=sum(r['measurements']['baseline']['score']==r['measurements']['candidate']['score'] for r in same),
  candidate_deeper=sum(r['measurements']['candidate']['completed_depth']>r['measurements']['baseline']['completed_depth'] for r in group),
  baseline_deeper=sum(r['measurements']['candidate']['completed_depth']<r['measurements']['baseline']['completed_depth'] for r in group))
(a.out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary,indent=2))
