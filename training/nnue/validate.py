"""Check exported models independently before tournament admission; write a readiness marker."""
import argparse,hashlib,json,subprocess
from pathlib import Path
import numpy as np
from quantized import Quantized
WIDTHS=[512,768,1024,1536,2048]
def digest(path):
 h=hashlib.sha256()
 with path.open('rb') as f:
  for x in iter(lambda:f.read(1<<20),b''):h.update(x)
 return h.hexdigest()
def main():
 p=argparse.ArgumentParser();p.add_argument('dataset',type=Path);p.add_argument('models',type=Path);p.add_argument('--binary',type=Path,default=Path('target/release/nnue_tool'));p.add_argument('--widths',nargs='+',type=int,default=WIDTHS);p.add_argument('--search-ms',type=int,default=3000);a=p.parse_args()
 schema=json.loads((a.dataset/'schema.json').read_text());samples=[json.loads(x) for x in (a.dataset/'samples.jsonl').read_text().splitlines()];val=[s for s in samples if s['split']=='validation'];data=np.memmap(a.dataset/'features.bin',mode='r',dtype='<u4');results=[]
 # Spread checks through validation rather than only the opening of the first game.
 selected=[val[i] for i in np.linspace(0,len(val)-1,min(8,len(val)),dtype=int)]
 for width in a.widths:
  checkpoint=a.models/f'NNUE_W{width}_v1.json';cp=json.loads(checkpoint.read_text());descriptor=cp['weights']['nnue'];blob=a.models/descriptor['file'];assert digest(blob)==descriptor['sha256'];assert descriptor['width']==width
  net=Quantized(blob,schema['features']);errors=[];baseline=[];predictions=[]
  for s in val:
   begin=s['offset'];middle=begin+s['us'];end=middle+s['them'];pred=net.residual(data[begin:middle],data[middle:end]);target=np.clip(s['score']-s['material'],-100000,100000)
   errors.append(pred-target);baseline.append(target);predictions.append(pred)
  error=np.array(errors)/1000;base=np.array(baseline)/1000
  def huber(x):return float(np.mean(np.where(np.abs(x)<1,.5*x*x,np.abs(x)-.5)))
  quality=dict(huber=huber(error),material_huber=huber(base),mae=float(np.mean(np.abs(errors))),material_mae=float(np.mean(np.abs(baseline))),residual_std=float(np.std(predictions)),positions=len(val))
  assert quality['residual_std']>1,'Network correction is effectively constant'
  assert quality['huber']<quality['material_huber'],'Held-out loss does not improve on material baseline'
  checks=[]
  for s in selected:
   raw=subprocess.run([str(a.binary), 'evaluate',str(checkpoint),s['game'],str(s['ply']),str(a.search_ms)],check=True,capture_output=True,text=True,timeout=60)
   result=json.loads(raw.stdout);begin=s['offset'];middle=begin+s['us'];end=middle+s['them'];expected=round(s['material'])+net.residual(data[begin:middle],data[middle:end]);assert abs(result['score']-expected)<=1,(result['score'],expected)
   assert result['search']['legal'] and result['search']['depth']>=1,result
   checks.append(dict(game=s['game'],ply=s['ply'],rust_score=result['score'],reference_score=expected,search=result['search']))
  results.append(dict(width=width,checkpoint=str(checkpoint),checkpoint_sha256=digest(checkpoint),blob_sha256=descriptor['sha256'],quality=quality,checks=checks));print(json.dumps(results[-1]),flush=True)
  del net
 readiness=dict(version=1,feature_hash=schema['hash'],widths=results)
 target=a.models/'ready.json';tmp=target.with_suffix('.tmp');tmp.write_text(json.dumps(readiness,indent=2)+'\n');tmp.replace(target)
if __name__=='__main__':main()
