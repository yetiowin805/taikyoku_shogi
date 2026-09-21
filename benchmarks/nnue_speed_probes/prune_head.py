#!/usr/bin/env python3
"""Calibrate small-head ablations on train games; assess on separate validation games.

This freezes all trained weights. Omitted first-dense neurons are replaced by
rounded calibration means. Greedy removal minimizes distortion from the original
network, not label error. It does not estimate the strength of a retrained model.
"""
import argparse, hashlib, json, os, pathlib, random, sys
os.environ['OPENBLAS_NUM_THREADS']='1'
import numpy as np
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[2]/'training/nnue'))
from quantized import Quantized
p=argparse.ArgumentParser();p.add_argument('--root',type=pathlib.Path,required=True);p.add_argument('--out',type=pathlib.Path,required=True);p.add_argument('--calibration',type=int,default=256);p.add_argument('--validation',type=int,default=512);a=p.parse_args();a.out.mkdir(parents=True,exist_ok=True)
ds=a.root/'data/nnue-training/dataset-v1';samples=[json.loads(x) for x in (ds/'samples.jsonl').read_text().splitlines()];idx=np.memmap(ds/'features.bin',dtype='<u4',mode='r')
def choose(split,n):
 rng=random.Random(721);games={}
 for s in samples:
  if s['split']==split: games.setdefault(s['game'],[]).append(s)
 groups=list(games.values());rng.shuffle(groups)
 for g in groups:rng.shuffle(g)
 chosen=[]
 while len(chosen)<n and any(groups):
  for g in groups:
   if g and len(chosen)<n:chosen.append(g.pop())
 return chosen
cal=choose('train',a.calibration);val=choose('validation',a.validation)
assert not ({s['game'] for s in cal}&{s['game'] for s in val})
(a.out/'quality-corpus.json').write_text(json.dumps({'dataset':json.loads((ds/'dataset.json').read_text()),'calibration':cal,'validation':val},indent=2))
def finish(net,h):
 h2=np.clip((h@net.h2.astype(np.int32).T+net.b2+32)//64,0,127)
 v=np.full(len(h),net.out_bias,dtype=np.float32)
 for j in range(32):v+=h2[:,j].astype(np.float32)/np.float32(127)*net.out[j]
 z=v*np.float32(1000)
 return np.clip(np.where(z>=0,np.floor(z+.5),np.ceil(z-.5)),-100000,100000).astype(np.int32)
def summarize(z,base,group):
 delta=z-base;ad=np.abs(delta);target=np.array([s['score'] for s in group]);material=np.array([s['material'] for s in group]);rounded=np.where(material>=0,np.floor(material+.5),np.ceil(material-.5));total=rounded+z;old=rounded+base
 # Labels are saved search scores, not ground-truth game values. This matches
 # the existing training target's clipping in residual score units.
 target_res=np.clip(target-material,-100000,100000)
 return {'n':len(z),'mean_abs_delta':float(ad.mean()),'p95_abs_delta':float(np.quantile(ad,.95)),'max_abs_delta':int(ad.max()),'rms_delta':float(np.sqrt(np.mean(delta.astype(float)**2))),'mean_delta':float(delta.mean()),'sign_changes':int(np.sum((total*old)<0)),'sign_changes_old_abs_gt_100':int(np.sum(((total*old)<0)&(np.abs(old)>100))),'original_target_mae':float(np.mean(np.abs(base-target_res))),'variant_target_mae':float(np.mean(np.abs(z-target_res))),'original_target_rmse':float(np.sqrt(np.mean((base-target_res)**2))),'variant_target_rmse':float(np.sqrt(np.mean((z-target_res)**2)))}
reports=[]
for width in [512,768,1024,1536,2048]:
 cp_path=a.root/f'data/nnue-admission-v2/NNUE_W{width}_v2.json';cp=json.loads(cp_path.read_text());desc=cp['weights']['nnue'];net=Quantized(cp_path.parent/desc['file'],238464)
 hidden=[];ranges=[]
 for s in cal+val:
  ids=idx[s['offset']:s['offset']+s['us']+s['them']]
  acc=np.concatenate([net.weights[part].sum(axis=0,dtype=np.int32)+net.bias for part in (ids[:s['us']],ids[s['us']:])])
  ranges.append([int(acc.min()),int(acc.max())])
  x=np.clip((acc.astype(np.int64)*127+2048)//4096,0,127).astype(np.int32)
  hidden.append(np.clip((net.h1.astype(np.int32)@x+net.b1+32)//64,0,127))
 h=np.array(hidden,dtype=np.int32);hc=h[:len(cal)];hv=h[len(cal):];basec=finish(net,hc);basev=finish(net,hv);fill=np.rint(hc.mean(axis=0)).astype(np.int32)
 keep=list(range(32));cur=hc.copy();plans={};trace=[]
 while len(keep)>16:
  choices=[]
  for j in keep:
   test=cur.copy();test[:,j]=fill[j];delta=finish(net,test).astype(float)-basec
   choices.append((float(np.mean(delta**2)),j))
  mse,j=min(choices);keep.remove(j);cur[:,j]=fill[j];trace.append({'removed':j,'calibration_mse':mse})
  if len(keep) in (24,16):
   k=len(keep);plan={'sha256':desc['sha256'],'width':width,'keep':keep.copy(),'fill':fill.tolist()};plans[k]=plan
   (a.out/f'W{width}-head{k}.json').write_text(json.dumps(plan,indent=2))
   test=hv.copy();dropped=sorted(set(range(32))-set(keep));test[:,dropped]=fill[dropped];z=finish(net,test)
   metrics=summarize(z,basev,val)
   reports.append({'width':width,'head':k,'model_sha256':desc['sha256'],'calibration_games':len({s['game'] for s in cal}),'validation_games':len({s['game'] for s in val}),'calibration':summarize(finish(net,cur),basec,cal),'validation':metrics,'accumulator_range':[min(v[0] for v in ranges),max(v[1] for v in ranges)]})
   print('W',width,'head',k,json.dumps(metrics),flush=True)
   with (a.out/f'W{width}-head{k}-validation.jsonl').open('w') as f:
    for sample,b,v in zip(val,basev,z):f.write(json.dumps({'game':sample['game'],'ply':sample['ply'],'original':int(b),'variant':int(v),'material':sample['material'],'teacher':sample['score']})+'\n')
 (a.out/f'W{width}-pruning-trace.json').write_text(json.dumps(trace,indent=2))
(a.out/'quality-summary.json').write_text(json.dumps({'reports':reports,'calibration_samples':len(cal),'validation_samples':len(val),'method':'Greedy mean replacement, no retraining. Validation held out from pruning fit, but used previously for original training early stopping. Search-score labels are imperfect teachers. Timings measured separately.'},indent=2))
