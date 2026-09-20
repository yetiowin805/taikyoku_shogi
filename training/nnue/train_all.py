"""Train the five widths sequentially; validate before publishing ready.json."""
import argparse,json,subprocess,sys,time
from pathlib import Path
HERE=Path(__file__).resolve().parent
p=argparse.ArgumentParser();p.add_argument('dataset',type=Path);p.add_argument('--base',type=Path,required=True);p.add_argument('--out',type=Path,required=True);p.add_argument('--epochs',type=int,default=2);p.add_argument('--threads',type=int,default=4);a=p.parse_args();a.out.mkdir(parents=True,exist_ok=True)
for width in [512,768,1024,1536,2048]:
 command=[sys.executable,str(HERE/'train.py'),str(a.dataset),'--base',str(a.base),'--out',str(a.out),'--width',str(width),'--epochs',str(a.epochs),'--threads',str(a.threads)]
 with (a.out/f'w{width}.log').open('x') as log:subprocess.run(command,stdout=log,stderr=subprocess.STDOUT,check=True)
 print(json.dumps(dict(completed_width=width,time=time.time())),flush=True)
subprocess.run([sys.executable,str(HERE/'validate.py'),str(a.dataset),str(a.out)],check=True)
