"""Sequential, capped full-engine screens. Restores prototype source on exit."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import time
import variants
ROOT=Path(__file__).resolve().parents[2];HERE=Path(__file__).resolve().parent
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('variants',nargs='+');p.add_argument('--out',type=Path,required=True)
p.add_argument('--cases',type=Path,required=True);p.add_argument('--corpus',type=Path,required=True)
p.add_argument('--target',type=Path,required=True);p.add_argument('--cpu',type=int,default=11)
p.add_argument('--max-wall',type=int,default=2400);p.add_argument('--repeats',type=int,default=2)
a=p.parse_args();start=time.monotonic();a.out.mkdir(parents=True,exist_ok=True)
def run(label,command,timeout):
    remaining=a.max_wall-(time.monotonic()-start)
    if remaining<30:raise TimeoutError('session budget exhausted')
    command=[sys.executable,str(HERE/'limit.py'),'--cpu',str(a.cpu),'--duty','.25',
             '--timeout',str(min(timeout,remaining)),'--stats',str(a.out/(label+'-resource.json')),'--',*map(str,command)]
    with (a.out/(label+'.log')).open('w') as f:
        result=subprocess.run(command,cwd=ROOT,stdout=f,stderr=subprocess.STDOUT)
    if result.returncode:raise RuntimeError(f'{label} failed ({result.returncode}); inspect log')
try:
    for variant in a.variants:
        if a.max_wall-(time.monotonic()-start)<240:
            print('Not starting another variant near time budget',flush=True);break
        print('BUILD',variant,flush=True)
        variants.apply(ROOT,variant)
        patch=subprocess.check_output(['git','diff','--',*variants.FILES],cwd=ROOT)
        (a.out/(variant+'.patch')).write_bytes(patch)
        try:
            run('build-'+variant,['cargo','build','--offline','--locked','--release','--example','speed_experiment','-j','1','--target-dir',a.target],900)
            binary=a.out/variant;shutil.copyfile(a.target/'release/examples/speed_experiment',binary);binary.chmod(0o755)
            print('MEASURE',variant,flush=True)
            run('measure-'+variant,[sys.executable,HERE/'run_pairs.py','--baseline',a.out/'baseline',
                '--candidate',binary,'--corpus',a.corpus,'--cases',a.cases,'--cpu',a.cpu,
                '--repeats',a.repeats,'--variant',variant,'--out',a.out/('pairs-'+variant)],1200)
            print('RESULT',variant,(a.out/('pairs-'+variant)/'summary.json').read_text(),flush=True)
        except (RuntimeError,TimeoutError) as e:
            print('FAILED',str(e),flush=True)
            (a.out/(variant+'-failure.txt')).write_text(str(e)+'\n')
finally:
    variants.apply(ROOT,'baseline')
