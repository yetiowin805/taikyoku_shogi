#!/usr/bin/env python3
"""Cross-check NumPy ablations against Rust; bootstrap paired error by game."""
import argparse,json,math,os,pathlib,subprocess
os.environ['OPENBLAS_NUM_THREADS']='1'
import numpy as np
p=argparse.ArgumentParser();p.add_argument('--root',type=pathlib.Path,required=True);p.add_argument('--out',type=pathlib.Path,required=True);p.add_argument('--binary',type=pathlib.Path,required=True);a=p.parse_args()
reports=[];rust=[]
for w in [512,768,1024,1536,2048]:
 for head in [24,16]:
  rows=[json.loads(x) for x in (a.out/f'W{w}-head{head}-validation.jsonl').read_text().splitlines()]
  grouped={}
  for r in rows:
   target=np.clip(r['teacher']-r['material'],-100000,100000)
   group=grouped.setdefault(r['game'],[0,0.,0.]);group[0]+=1;group[1]+=abs(r['original']-target);group[2]+=abs(r['variant']-target)
  g=np.array(list(grouped.values()));rng=np.random.default_rng(982)
  ids=rng.integers(0,len(g),size=(2000,len(g)));b=g[ids].sum(axis=1)
  delta=(b[:,2]-b[:,1])/b[:,0];relative=(b[:,2]/b[:,1]-1)*100
  reports.append({'width':w,'head':head,'games':len(g),'mae_delta_95pct_cluster_bootstrap':np.quantile(delta,[.025,.975]).tolist(),'mae_relative_pct_95pct_cluster_bootstrap':np.quantile(relative,[.025,.975]).tolist()})
  for r in rows[:2]:
   env=os.environ.copy();env['NNUE_SPEED_PROBE']='both';env['NNUE_FOLLOWUP']=f'quant,packed,royal,memo,head{head}';env['NNUE_HEAD_PLAN']=str(a.out/f'W{w}-head{head}.json')
   result=subprocess.run(['taskset','-c','0',str(a.binary),str(a.root/f'data/nnue-admission-v2/NNUE_W{w}_v2.json'),r['game'],str(r['ply']),'3000','1','static'],env=env,text=True,capture_output=True,check=True,timeout=60)
   material=r['material'];mr=math.floor(material+.5) if material>=0 else math.ceil(material-.5);expected=mr+r['variant'];actual=json.loads(result.stdout)['static_score']
   assert expected==actual,(w,head,r['game'],r['ply'],expected,actual)
   rust.append({'width':w,'head':head,'game':r['game'],'ply':r['ply'],'score':actual})
 print('checked width',w,flush=True)
(a.out/'quality-checks.json').write_text(json.dumps({'checks':rust,'all_exact':True,'bootstrap':reports,'note':'Paired bootstrap resamples validation games, conditional on these calibrated pruning plans. Labels were also used for original early stopping; this is not a new independent strength test.'},indent=2))
