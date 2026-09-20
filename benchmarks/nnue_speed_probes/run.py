#!/usr/bin/env python3
"""Small local study; run after both builds and correctness tests finish."""
import argparse,datetime,hashlib,json,os,pathlib,random,subprocess,time
p=argparse.ArgumentParser();p.add_argument('--root',type=pathlib.Path,required=True);p.add_argument('--out',type=pathlib.Path,required=True);p.add_argument('--generic',type=pathlib.Path,required=True);p.add_argument('--native',type=pathlib.Path,required=True);p.add_argument('--phase',choices=['verify','micro','timed'],required=True);p.add_argument('--widths',default='512,2048');p.add_argument('--variants',default='baseline,native,fused,snapshot,both,native-both');p.add_argument('--reps',type=int,default=2);p.add_argument('--verify-depth',type=int,default=1);p.add_argument('--positions',default='opening,middlegame,tactical,long-history');a=p.parse_args();a.out.mkdir(parents=True,exist_ok=True)
prior=json.loads((a.root/'data/nnue-speed-probe-20260920/manifest.json').read_text())
positions=[p for p in prior['positions'] if p['name'] in a.positions.split(',')];widths=[int(w) for w in a.widths.split(',')];variants=a.variants.split(',')
models={str(w):(a.root/f'data/nnue-admission-v2/NNUE_W{w}_v2.json' if w else a.root/'data/nnue-speed-probe-20260920/models/BASE_C2S2_A0_L0.json') for w in widths}
variants_info={'baseline':(a.generic,'baseline'),'native':(a.native,'baseline'),'fused':(a.generic,'fused'),'snapshot':(a.generic,'snapshot'),'both':(a.generic,'both'),'native-both':(a.native,'both')}
def sha(path):
 with open(path,'rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
manifest={'created':datetime.datetime.now(datetime.timezone.utc).isoformat(),'phase':a.phase,'revision':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'diff':subprocess.check_output(['git','diff'],text=True),'source_sha256':{str(p):sha(p) for p in [pathlib.Path(x) for x in ['Cargo.toml','src/nnue/mod.rs','src/nnue/features.rs','src/nnue/experiment.rs','src/game_state.rs','examples/nnue_speed_probe.rs','benchmarks/nnue_speed_probes/run.py']]},'cpu':0,'time_ms':3000,'reps':a.reps,'verify_depth':a.verify_depth,'positions':positions,'models':{k:{'path':str(v),'sha256':sha(v),'checkpoint':json.loads(v.read_text())} for k,v in models.items()},'binaries':{str(b):sha(b) for b in [a.generic,a.native]},'hardware':subprocess.check_output(['lscpu'],text=True),'compiler':subprocess.check_output(['rustc','-Vv'],text=True)}
(a.out/f'{a.phase}-manifest.json').write_text(json.dumps(manifest,indent=2))
order=[];rng=random.Random(29092026)
for rep in range(a.reps if a.phase=='timed' else 1):
 blocks=[(w,pos) for w in widths for pos in (positions if a.phase!='micro' else positions[:1])];rng.shuffle(blocks)
 for w,pos in blocks:
  vs=variants[:];rng.shuffle(vs)
  for v in vs:order.append((rep,w,pos,v))
(a.out/f'{a.phase}-order.json').write_text(json.dumps(order,indent=2))
with (a.out/f'{a.phase}.jsonl').open('w') as f:
 for i,(rep,w,pos,v) in enumerate(order):
  binary,mode=variants_info[v];env=os.environ.copy();env['NNUE_SPEED_PROBE']=mode
  limit,depth=(30000,a.verify_depth) if a.phase=='verify' else (3000,8)
  cmd=['/usr/bin/time','-f','PEAK_RSS_KIB=%M','taskset','-c','0',str(binary),str(models[str(w)]),pos['game'],str(pos['ply']),str(limit),str(depth),'micro' if a.phase=='micro' else 'search']
  start=time.monotonic();r=subprocess.run(cmd,env=env,capture_output=True,text=True,timeout=45)
  (a.out/f'{a.phase}-{i:03d}.stderr').write_text(r.stderr)
  if r.returncode:raise RuntimeError(r.stderr)
  result=json.loads(r.stdout);record={'index':i,'rep':rep,'width':w,'position':pos['name'],'variant':v,'wall_seconds':time.monotonic()-start,'peak_rss_kib':int(r.stderr.split('PEAK_RSS_KIB=')[-1].strip()),'result':result}
  if a.phase!='micro':
   assert result['legal'],record
   if a.phase=='verify':assert not result['aborted'] and result['depth']==depth,record
  f.write(json.dumps(record)+'\n');f.flush()
  print(f'{a.phase} {i+1}/{len(order)} W{w} {pos["name"]} {v}: '+(str(round(result['nodes']/(result['elapsed_ns']/1e9)))+' nps, depth '+str(result['depth']) if 'nodes' in result else 'micro complete'),flush=True)
if a.phase=='verify':
 rows=[json.loads(x) for x in (a.out/'verify.jsonl').read_text().splitlines()]
 fields=['depth','nodes','qnodes','score','static_score','best','root_lines']
 for row in rows:
  base=next(x for x in rows if x['width']==row['width'] and x['position']==row['position'] and x['variant']=='baseline')
  for field in fields:assert row['result'][field]==base['result'][field],(row['width'],row['position'],row['variant'],field)
 print('All fixed-depth scores, full routes, ordered root lines, node counts and qnodes match.',flush=True)
