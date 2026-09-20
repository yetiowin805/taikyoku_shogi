"""Pinned, sequential randomized pairs. No compiler runs during recorded comparisons."""
import argparse,json,os,pathlib,random,select,subprocess,time
ROOT=pathlib.Path(__file__).resolve().parents[2];OUT=ROOT/'data/derived/search-speed-20260920'
class Server:
 def __init__(self,variant,label,cpu):
  self.log=(OUT/f'{label}-{variant}.stderr').open('w')
  env={k:v for k,v in os.environ.items() if not k.startswith('TAIKYOKU_AB_')}
  self.p=subprocess.Popen(['taskset','-c',str(cpu),str(OUT/'bin'/variant),str(OUT/'corpus.json')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.log,text=True,bufsize=1,env=env)
 def request(self,case,budget=0):
  q=dict(position=case['position'],model=case['model'],depth=case['depth'],budget_ms=budget)
  self.p.stdin.write(json.dumps(q)+'\n');self.p.stdin.flush()
  ready,_,_=select.select([self.p.stdout],[],[],120)
  if not ready:raise TimeoutError('benchmark process did not return in 120s')
  line=self.p.stdout.readline()
  if not line:raise RuntimeError('benchmark process exited; inspect stderr log')
  result=json.loads(line)
  result['pid']=self.p.pid
  status=pathlib.Path(f'/proc/{self.p.pid}/status')
  if status.exists():
   memory={line.split(':',1)[0]:int(line.split()[1]) for line in status.read_text().splitlines() if line.startswith(('VmRSS:','VmHWM:'))}
   result['rss_kib']=memory.get('VmRSS');result['peak_rss_kib']=memory.get('VmHWM')
  return result
 def close(self):
  if self.p.poll() is None:
   self.p.stdin.close()
   try:self.p.wait(timeout=10)
   except subprocess.TimeoutExpired:self.p.kill();self.p.wait()
  self.log.close()
def compare(a,b):
 if a['signature']!=b['signature']:
  raise AssertionError(f'Fixed-depth search mismatch: {a} vs {b}')
 if a['aborted'] or b['aborted']:raise AssertionError('fixed-depth search aborted')

def main():
 p=argparse.ArgumentParser();p.add_argument('phase',choices=['pilot','screen','confirm']);p.add_argument('--variants',nargs='+',default=['qcache','iter','pool0','tt-clear','tt-dirty']);p.add_argument('--repeats',type=int);p.add_argument('--cpu',type=int,default=2);p.add_argument('--label');p.add_argument('--restart-every',type=int,default=0);args=p.parse_args()
 corpus=json.loads((OUT/'corpus.json').read_text());label=args.label or args.phase
 if args.phase=='pilot':
  server=Server('stock',label,args.cpu);cases=[]
  existing=json.loads((OUT/'cases.json').read_text()) if (OUT/'cases.json').exists() else []
  known={(c['position'],c['model']):c for c in existing}
  try:
   for pi in range(len(corpus['positions'])):
    for mi in range(len(corpus['models'])):
     if (pi,mi) in known:
      cases.append(known[(pi,mi)]);continue
     q=dict(position=pi,model=mi,depth=4);r=server.request(q,1000)
     case=dict(position=pi,model=mi,depth=min(3,max(1,r['completed_depth'])),pilot=r);cases.append(case)
     print('PILOT',pi,r['name'],r['agent'],'fixed depth',case['depth'],flush=True)
  finally:server.close()
  (OUT/'cases.json').write_text(json.dumps(cases,indent=2)+'\n');return
 cases=json.loads((OUT/'cases.json').read_text())
 if args.phase=='screen':cases=[c for c in cases if c['position']<4]
 repeats=args.repeats or (3 if args.phase=='screen' else 16)
 assert args.phase!='confirm' or len(args.variants)==1,'Confirm one selected candidate'
 plan=dict(phase=args.phase,label=label,variants=args.variants,repeats=repeats,cpu=args.cpu,restart_every=args.restart_every,
   cases=[{k:c[k] for k in ['position','model','depth']} for c in cases],seed=20260921,
   primary='held-out positions only: equal-position/equal-agent geometric mean paired wall-time ratio; position-cluster 95% CI and exact two-sided sign-flip test',
   holdout_positions=sorted({c['position'] for c in cases if c['position']>=4}),created=time.time())
 (OUT/f'{label}-plan.json').write_text(json.dumps(plan,indent=2)+'\n')
 for variant in args.variants:
  servers={v:Server(v,label,args.cpu) for v in ['stock',variant]}
  try:
   # Exact-depth warmups prime code, NNUE pages, and optional reusable storage.
   for case in cases:
    a=servers['stock'].request(case);b=servers[variant].request(case);compare(a,b)
   print('WARMED',variant,len(cases),'matched cases',flush=True)
   rng=random.Random(plan['seed']);starts={(c['position'],c['model']):rng.randrange(2) for c in cases}
   with (OUT/f'{label}-{variant}.jsonl').open('w') as f:
    for rep in range(repeats):
     if args.restart_every and rep and rep % args.restart_every == 0:
      for server in servers.values():server.close()
      servers={v:Server(v,label+f'-restart-{rep}',args.cpu) for v in ['stock',variant]}
      for case in cases:
       a=servers['stock'].request(case);b=servers[variant].request(case);compare(a,b)
      print('RESTARTED',variant,'before rep',rep+1,flush=True)
     order=cases.copy();rng.shuffle(order)
     for case in order:
      which=['stock',variant]
      if (rep+starts[(case['position'],case['model'])])%2:which.reverse()
      measurements={v:servers[v].request(case) for v in which}
      compare(measurements['stock'],measurements[variant])
      row=dict(rep=rep,position=case['position'],model=case['model'],depth=case['depth'],order=which,measurements=measurements)
      f.write(json.dumps(row)+'\n');f.flush()
     print('MEASURED',variant,'rep',rep+1,'/',repeats,flush=True)
  finally:
   for s in servers.values():s.close()
 (OUT/f'{label}-finished.json').write_text(json.dumps(dict(finished=time.time(),plan=plan)))
if __name__=='__main__':main()
