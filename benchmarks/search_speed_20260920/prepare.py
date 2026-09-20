"""Freeze existing, already-trained models and replayable position identities."""
import hashlib,json,pathlib,subprocess,shutil,os
ROOT=pathlib.Path(__file__).resolve().parents[2]
OUT=ROOT/'data/derived/search-speed-20260920';OUT.mkdir(parents=True,exist_ok=True)
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
old=json.loads((ROOT/'data/nnue-speed-probe-20260920/manifest.json').read_text())
positions=old['positions']
sanity=json.loads((ROOT/'data/nnue-training/sanity-positions.json').read_text())
for i,name in zip([0,1,2,6,3,4,5,7], ['early-development','midgame-independent','late-independent','late-royal','champs-midgame','twins-midgame-a','twins-midgame-b','late-endgame']):
 p=sanity[i];positions.append(dict(name=name,game=p['game'],ply=p['ply']))
inputs=OUT/'inputs';inputs.mkdir(exist_ok=True)
for i,p in enumerate(positions):
 source=pathlib.Path(p['game']);dest=inputs/f'position-{i:02d}.json'
 shutil.copyfile(source,dest);p['source_game']=str(source);p['game']=str(dest);p['sha256']=sha(dest)
models=[]
for key in ['BASE_C2S2_A40_Lmate','C2K50A1','NNUE512','NNUE2048']:
 p=pathlib.Path(old['models'][key]['path']); d=json.loads(p.read_text())
 snapshot=inputs/p.name;shutil.copyfile(p,snapshot)
 item=dict(name=key,path=str(snapshot),source_model=str(p),sha256=sha(snapshot),search_defaults=d['search_defaults'])
 if nnue:=d['weights'].get('nnue'):
  blob=p.parent/nnue['file'];actual=sha(blob);assert actual==nnue['sha256'];item['nnue']=nnue
  dest=inputs/nnue['file']
  if not dest.exists():os.link(blob,dest)
  assert sha(dest)==actual
 models.append(item)
d=dict(revision='aee552ee96416f3b6d5471e8f2498a64742eec03',tooling_revision=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),positions=positions,models=models)
(OUT/'corpus.json').write_text(json.dumps(d,indent=2)+'\n')
print('Frozen',len(positions),'positions x',len(models),'agents')
