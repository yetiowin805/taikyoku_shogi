#!/usr/bin/env python3
"""Caller must hold /tmp/nnue-comprehensive/cpu.lock. Census is diagnostic only."""
import argparse,json,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',required=True,type=Path);p.add_argument('--corpus',required=True,type=Path);p.add_argument('--cases',required=True,type=Path);p.add_argument('--out',required=True,type=Path);p.add_argument('--models',type=int,nargs='+');p.add_argument('--cpu',type=int,default=2);a=p.parse_args()
case_list=[c for c in json.loads(a.cases.read_text()) if a.models is None or c['model'] in a.models]
with open(str(a.out)+'.stderr','w') as err, a.out.open('w') as out:
 proc=subprocess.Popen(['taskset','-c',str(a.cpu),str(a.binary),str(a.corpus)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err,text=True)
 try:
  for c in case_list:
   req={k:c[k] for k in ['position','model','depth']}
   proc.stdin.write(json.dumps(req)+'\n');proc.stdin.flush()
   line=proc.stdout.readline();assert line,(proc.poll(),str(a.out)+'.stderr');row=json.loads(line);assert row['signature']
   out.write(json.dumps(row)+'\n');out.flush()
 finally:
  proc.stdin.close();proc.wait();assert proc.returncode==0
print(str(a.out),len(case_list),'cases')
