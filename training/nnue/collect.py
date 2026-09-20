"""Run from the repository root; stream a reproducible archive of completed games.
python3 training/nnue/collect.py > corpus.tar.gz
Only completed immutable game files are copied; no tournament files are modified.
"""
import argparse,hashlib,io,json,random,sys,tarfile
from pathlib import Path

def main():
 p=argparse.ArgumentParser();p.add_argument('--per-run',type=int,default=300);p.add_argument('--seed',type=int,default=20260920)
 p.add_argument('--runs',nargs='+',default=['royal-s2-swiss-20260828T064125Z','royal-s2-champs-swiss-20260902T*','royal-s2-twins-swiss-20260904T070539Z','royal-al-32-20260914T054159Z'])
 a=p.parse_args();rng=random.Random(a.seed);manifest={'version':1,'seed':a.seed,'games':[],'runs':{}}
 def put(tar,name,data):
  info=tarfile.TarInfo(name);info.size=len(data);info.mtime=0;tar.addfile(info,io.BytesIO(data))
 with tarfile.open(fileobj=sys.stdout.buffer,mode='w|gz',compresslevel=1) as tar:
  seen=set()
  for pattern in a.runs:
   dirs=list(Path('data/raw/tourney').glob(pattern));assert len(dirs)==1,(pattern,dirs)
   run=dirs[0];state=json.loads((run/'state.json').read_text());slots=[x for x in state['slots'] if x['status']=='done' and x.get('game_path')];rng.shuffle(slots)
   n=0
   for slot in slots:
    path=Path(slot['game_path']);data=path.read_bytes();g=json.loads(data)
    if not g.get('result') or g.get('abort_reason'):continue
    trajectory=json.dumps([g['start'],[{k:v for k,v in m.items() if k in ['color','from_file','from_rank','to_file','to_rank','promoted','data']} for m in g['moves']]],sort_keys=True).encode()
    unique=hashlib.sha256(trajectory).hexdigest()
    if unique in seen:continue
    seen.add(unique);name=f'games/{run.name}/{path.name}';put(tar,name,data)
    prefix=json.dumps([g['start'],json.loads(trajectory)[1][:64]],sort_keys=True).encode();group=hashlib.sha256(prefix).hexdigest()
    manifest['games'].append(dict(path=name,source=str(path),sha256=hashlib.sha256(data).hexdigest(),group=group,split='validation' if int(group[:8],16)%10==0 else 'train'))
    n+=1
    if n>=a.per_run:break
   manifest['runs'][run.name]=n
  base=Path('models/royal-al-grid/BASE_C2S2_A0_L0.json').read_bytes();put(tar,'base.json',base);manifest['base_sha256']=hashlib.sha256(base).hexdigest()
  put(tar,'manifest.json',json.dumps(manifest,indent=2).encode())
if __name__=='__main__':main()
