#!/usr/bin/env python3
"""Replace exactly five Lold entries in a NEW field; never edit a live run/state."""
import argparse,hashlib,json,subprocess
from pathlib import Path
WIDTHS=[512,768,1024,1536,2048]
def digest(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  for b in iter(lambda:f.read(1<<20),b''):h.update(b)
 return h.hexdigest()
def prepare(source,models,out,validator):
 if out.exists():raise ValueError('Output manifest already exists')
 entrants=json.loads(source.read_text())['entrants'];assert len(entrants)==32
 removed=[e for e in entrants if e['id'].endswith('_Lold')];assert len(removed)==5
 kept=[e for e in entrants if e not in removed]
 for e in kept:
  if not Path(e['model']).is_file():raise ValueError('Missing retained model: '+e['model'])
  if e.get('engine') and not Path(e['engine']).is_file():raise ValueError('Missing retained engine: '+e['engine'])
  subprocess.run([str(validator),'--validate-model',e['model']],check=True)
 baseline=None
 ready=json.loads((models/'ready.json').read_text());assert sorted(r['width'] for r in ready['widths'])==WIDTHS
 for width in WIDTHS:
  cp_path=models/f'NNUE_W{width}_v1.json';cp=json.loads(cp_path.read_text());d=cp['weights']['nnue'];r=next(r for r in ready['widths'] if r['width']==width)
  identity=(cp['weights']['piece'],cp['search_defaults'])
  if baseline is None:baseline=identity
  assert identity==baseline,'NNUE agents must share material and search settings'
  assert d['width']==width and digest(cp_path)==r['checkpoint_sha256'];assert digest(models/d['file'])==d['sha256']==r['blob_sha256']
  assert r['quality']['huber']<r['quality']['material_huber'] and len(r['checks'])>=4
  subprocess.run([str(validator),'--validate-model',str(cp_path)],check=True)
  kept.append(dict(id=cp['name'],model=str(cp_path)))
 assert len({e['id'] for e in kept})==32
 out.parent.mkdir(parents=True,exist_ok=True);temp=out.with_suffix('.tmp');temp.write_text(json.dumps(dict(entrants=kept),indent=2)+'\n');temp.replace(out)
 return dict(removed=[e['id'] for e in removed],added=[e['id'] for e in kept[-5:]])
def main():
 p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--models',type=Path,required=True);p.add_argument('--out',type=Path,required=True);p.add_argument('--validator',type=Path,default=Path('target/release/analyze_position'));a=p.parse_args()
 try:print(json.dumps(prepare(a.source,a.models,a.out,a.validator),indent=2))
 except Exception as e:raise SystemExit(f'prepare_nnue_field failed: {e}; tournament did not start')
if __name__=='__main__':main()
