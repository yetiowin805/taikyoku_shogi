"""Restore a corpus archive and freeze stratified, finite-score sample requests."""
import argparse,hashlib,json,random,tarfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('directory',type=Path);p.add_argument('--per-game',type=int,default=12);a=p.parse_args();root=a.directory.resolve()
with tarfile.open(root/'corpus.tar.gz') as t:
 for entry in t:
  dest=(root/entry.name).resolve()
  if not dest.is_relative_to(root) or not entry.isfile():raise ValueError('Invalid corpus member')
  data=t.extractfile(entry).read()
  if dest.exists():assert dest.read_bytes()==data
  else:dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(data)
m=json.loads((root/'manifest.json').read_text());jobs=[]
assert hashlib.sha256((root/'base.json').read_bytes()).hexdigest()==m['base_sha256']
for g in m['games']:
 path=root/g['path'];data=path.read_bytes();assert hashlib.sha256(data).hexdigest()==g['sha256']
 game=json.loads(data);valid=[i for i,r in enumerate(game['moves']) if i>=16 and isinstance(r.get('eval'),int) and abs(r['eval'])<900000]
 if not valid:continue
 rng=random.Random(int(g['sha256'][:16],16));n=min(a.per_game,len(valid));plies=[rng.choice(valid[i*len(valid)//n:(i+1)*len(valid)//n]) for i in range(n)]
 jobs.append(dict(game=str(path),plies=plies,split=g['split']))
(root/'jobs.jsonl').write_text(''.join(json.dumps(j)+'\n' for j in jobs))
print(json.dumps(dict(games=len(jobs),requested_positions=sum(len(j['plies']) for j in jobs),runs=m['runs'])))
