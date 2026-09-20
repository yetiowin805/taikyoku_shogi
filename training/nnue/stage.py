"""Stage only admitted immutable inference artifacts; omit raw games and optimizer states."""
import argparse,json,shutil,hashlib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('models',type=Path);p.add_argument('out',type=Path);a=p.parse_args()
if a.out.exists():raise SystemExit('Stage directory exists; choose a new one')
ready=json.loads((a.models/'ready.json').read_text());assert sorted(x['width'] for x in ready['widths'])==[512,768,1024,1536,2048]
a.out.mkdir(parents=True)
for item in ready['widths']:
 w=item['width'];cp=a.models/f'NNUE_W{w}_v1.json';assert hashlib.sha256(cp.read_bytes()).hexdigest()==item['checkpoint_sha256'];descriptor=json.loads(cp.read_text())['weights']['nnue']
 for source in [cp,a.models/descriptor['file'],a.models/f'w{w}.metrics.json']:
  target=a.out/source.name
  if not target.exists():shutil.copyfile(source,target)
shutil.copyfile(a.models/'ready.json',a.out/'ready.json')
print(a.out)
