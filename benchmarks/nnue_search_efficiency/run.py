#!/usr/bin/env python3
"""Serial paired experiments. Freeze requests and binary/input hashes for each group."""
import hashlib,json,os,pathlib,random,select,subprocess,sys,time
ROOT=pathlib.Path(__file__).resolve().parent
REPO=ROOT.parents[1]
VARIANTS={'stageb':{'stage_b_cached_sort':True},'base':{},'lazy':{'q_lazy_royal':True},'early':{'q_early_filter':True},'sort':{'q_cached_sort':True},
 'qfast':{'q_lazy_royal':True,'q_early_filter':True,'q_cached_sort':True},
 'guard':{'pv_tt_guard':True},'pvs1':{'pvs_min_depth':1},'pvs2':{'pvs_min_depth':2}}
def sha(p):return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
def power_state():
 paths=['/sys/class/power_supply/AC/online','/sys/devices/system/cpu/cpu2/cpufreq/scaling_cur_freq',
 '/sys/devices/system/cpu/cpu2/cpufreq/energy_performance_preference','/sys/class/thermal/thermal_zone7/temp']
 return dict(utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),readings={p:pathlib.Path(p).read_text().strip() for p in paths if pathlib.Path(p).exists()})
def req(p,v='base',depth=3,budget=60000,repeat=0):
 return dict(position=p,variant=v,depth=depth,budget_ms=budget,repeat=repeat,study=VARIANTS[v])
def run(name,requests,binary=None):
 binary=pathlib.Path(binary or ROOT/'bin/study').resolve()
 corpus=json.loads((ROOT/'corpus.json').read_text())
 for x in [*corpus['positions'],*corpus['models']]:
  p=x.get('game',x.get('path'));assert sha(p)==x['sha256'],p
 plan=dict(name=name,binary=str(binary),binary_sha256=sha(binary),corpus_sha256=sha(ROOT/'corpus.json'),cpu=2,requests=requests,runner_sha256=sha(__file__),initial_power=power_state())
 (ROOT/'results'/f'{name}-plan.json').write_text(json.dumps(plan,indent=2)+'\n')
 with (ROOT/'results'/f'{name}.jsonl').open('w') as out,(ROOT/'results'/f'{name}.log').open('w') as err:
  env={k:v for k,v in os.environ.items() if not k.startswith('TAIKYOKU_')}
  proc=subprocess.Popen(['taskset','-c','2','nice','-n','10',str(binary),str(ROOT/'corpus.json')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err,text=True,env=env)
  try:
   for i,q in enumerate(requests):
    before_power=power_state()
    if name=='timed' and before_power['readings'].get('/sys/class/power_supply/AC/online')!='1':raise RuntimeError('AC power lost; stop timed comparison')
    proc.stdin.write(json.dumps(q)+'\n');proc.stdin.flush()
    if not select.select([proc.stdout],[],[],max(120,q['budget_ms']/1000+60))[0]:raise TimeoutError(q)
    line=proc.stdout.readline()
    if not line:raise RuntimeError(f'engine exited: {q}; inspect {name}.log')
    d=json.loads(line);assert d['nnue'];d['power_before']=before_power;d['power_after']=power_state();out.write(json.dumps(d)+'\n');out.flush()
    print(name,i+1,len(requests),d['position'],d['variant'],d['completed_depth'],round(d['cpu_ms']),d['nodes'],flush=True)
   proc.stdin.close();assert proc.wait(timeout=10)==0
  finally:
   if proc.poll() is None:proc.kill();proc.wait()
if __name__=='__main__':
 mode=sys.argv[1]
 ps=[0,1,2,6,7,10,16,17,100,102,104,106]
 if mode=='identity':run(mode,[req(p) for p in ps],ROOT/'bin/baseline')
 elif mode=='screen':
  qs=[]
  for p in ps:
   vs=list(VARIANTS);random.Random(4510+p).shuffle(vs)
   qs.extend(req(p,v) for v in vs)
  run(mode,qs)
 elif mode=='profile':run(mode,[dict(req(p,depth=4),study={'profile':True}) for p in [0,2,6,10,16,104]],ROOT/'bin/profile')
 else:raise SystemExit('identity, screen, profile; later groups are frozen by their plan files')
