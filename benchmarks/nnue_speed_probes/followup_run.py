#!/usr/bin/env python3
"""Sequential, pinned, shuffled paired follow-ups to the accepted NNUE baseline."""
import argparse,datetime,hashlib,json,os,pathlib,random,subprocess,time
p=argparse.ArgumentParser();p.add_argument('--fixture',type=pathlib.Path);p.add_argument('--root',type=pathlib.Path,required=True);p.add_argument('--out',type=pathlib.Path,required=True);p.add_argument('--binary',type=pathlib.Path,required=True);p.add_argument('--phase',choices=['verify','timed','micro','counters','quality-search'],required=True);p.add_argument('--variants',default='legacy,control,quant,packed,royal,memo,exact,head24,head16');p.add_argument('--reps',type=int,default=2);p.add_argument('--depth',type=int,default=2);p.add_argument('--widths',default='512,2048');a=p.parse_args();a.out.mkdir(parents=True,exist_ok=True)
prior=json.loads((a.root/'data/nnue-speed-probe-20260920/manifest.json').read_text());positions=prior['positions'];widths=list(map(int,a.widths.split(',')));variants=a.variants.split(',')
flags={'legacy':'','control':'','quant':'quant','packed':'packed','royal':'royal','memo':'memo','exact':'quant,packed,royal,memo','recommended':'packed,royal','head24':'quant,packed,royal,memo,head24','head16':'quant,packed,royal,memo,head16'}
if a.fixture:positions=[dict(name='forced-royal-fixture',game=str(a.fixture.resolve()),ply=0)]
if a.phase=='quality-search':
 corpus=json.loads((a.out/'quality-corpus.json').read_text())
 positions=[dict(name='heldout-'+str(i),game=s['game'],ply=s['ply']) for i,s in enumerate(corpus['validation'][:8])]
if a.phase=='micro':positions=positions[:1]
def sha(p):
 with open(p,'rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
models={w:a.root/f'data/nnue-admission-v2/NNUE_W{w}_v2.json' for w in widths};legacy=a.root/'data/nnue-speed-experiments-20260920/bin/native'
manifest={'source_sha256':{str(p):sha(p) for p in [pathlib.Path(x) for x in ['src/nnue/mod.rs','src/nnue/experiment.rs','src/search.rs','src/game_state.rs','examples/nnue_speed_probe.rs','benchmarks/nnue_speed_probes/followup_run.py']]},'revision':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'diff':subprocess.check_output(['git','diff'],text=True),'created':datetime.datetime.now(datetime.timezone.utc).isoformat(),'cpu':0,'time_ms':3000,'micro_memo_disabled':a.phase=='micro','phase':a.phase,'depth':a.depth,'variants':variants,'flags':flags,'reps':a.reps,'positions':positions,'game_hashes':{p['game']:sha(p['game']) for p in positions},'models':{str(w):json.loads(m.read_text())['weights']['nnue'] for w,m in models.items()},'binary_sha256':sha(a.binary),'legacy_sha256':sha(legacy),'compiler':subprocess.check_output(['rustc','-Vv'],text=True),'hardware':subprocess.check_output(['lscpu'],text=True),'rustflags':'-C target-cpu=native','profile':'release; thin LTO; codegen-units=1'}
(a.out/f'{a.phase}-manifest.json').write_text(json.dumps(manifest,indent=2));order=[];rng=random.Random(392021)
for rep in range(a.reps if a.phase=='timed' else 1):
 blocks=[(w,p) for w in widths for p in positions];rng.shuffle(blocks)
 for w,pos in blocks:
  vs=variants.copy();rng.shuffle(vs)
  for v in vs:order.append((rep,w,pos,v))
(a.out/f'{a.phase}-order.json').write_text(json.dumps(order,indent=2));rows=[]
with (a.out/f'{a.phase}.jsonl').open('x') as f:
 for i,(rep,w,pos,v) in enumerate(order):
  env=os.environ.copy();env['NNUE_SPEED_PROBE']='both';env['NNUE_FOLLOWUP']=flags[v]+(',stats' if a.phase=='counters' else '')
  if a.phase=='micro':env['NNUE_FOLLOWUP']=','.join(x for x in env['NNUE_FOLLOWUP'].split(',') if x!='memo')
  if v.startswith('head'):env['NNUE_HEAD_PLAN']=str(a.out/f'W{w}-{v}.json')
  binary=legacy if v=='legacy' else a.binary
  limit,depth=(30000,a.depth) if a.phase in ('verify','quality-search','counters') else (3000,8)
  cmd=['/usr/bin/time','-f','PEAK_RSS_KIB=%M','taskset','-c','0',str(binary),str(models[w]),pos['game'],str(pos['ply']),str(limit),str(depth),'micro' if a.phase=='micro' else 'search']
  t=time.monotonic();r=subprocess.run(cmd,env=env,capture_output=True,text=True,timeout=60)
  (a.out/f'{a.phase}-{i:03d}.stderr').write_text(r.stderr)
  if r.returncode:raise RuntimeError(r.stderr)
  result=json.loads(r.stdout)
  if v!='legacy' and a.phase!='micro':assert 'counters' in result, 'Wrong executable: follow-up telemetry field missing'
  row={'index':i,'rep':rep,'width':w,'position':pos['name'],'game':pos['game'],'ply':pos['ply'],'variant':v,'wall_seconds':time.monotonic()-t,'peak_rss_kib':int(r.stderr.split('PEAK_RSS_KIB=')[-1].strip()),'result':result}
  if a.phase!='micro':
   assert result['legal'],row
   if a.phase in ('verify','quality-search','counters'):assert not result['aborted'] and result['depth']==depth,row
  f.write(json.dumps(row)+'\n');f.flush();rows.append(row)
  print(a.phase,f'{i+1}/{len(order)}',w,pos['name'],v, ('depth '+str(result['depth'])+'; '+str(round(result['nodes']/(result['elapsed_ns']/1e9)))+' nps' if 'nodes' in result else 'done'),flush=True)
if a.phase=='verify':
 for row in rows:
  if row['variant'].startswith('head'):continue
  base=next(b for b in rows if b['width']==row['width'] and b['position']==row['position'] and b['variant']=='control')
  for k in ['depth','score','static_score','nodes','qnodes','best','root_lines']:assert row['result'][k]==base['result'][k],(row['width'],row['position'],row['variant'],k)
 print('Exact variants match scores, full routes, ordered root lines and all node counts.',flush=True)
