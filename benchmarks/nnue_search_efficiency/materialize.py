#!/usr/bin/env python3
"""Point the frozen manifest at locally supplied games and the W512 checkpoint."""
import argparse,hashlib,json,pathlib
root=pathlib.Path(__file__).resolve().parent
ap=argparse.ArgumentParser(description=__doc__)
ap.add_argument('--games',type=pathlib.Path,default=root/'input/games')
ap.add_argument('--model',type=pathlib.Path)
args=ap.parse_args();p=root/'corpus.json';corpus=json.loads(p.read_text())
def verify(path,sha):
 if not path.is_file():raise SystemExit(f'Missing private input: {path}')
 if hashlib.sha256(path.read_bytes()).hexdigest()!=sha:raise SystemExit(f'Input checksum mismatch: {path}')
for item in corpus['positions']:
 path=args.games.resolve()/pathlib.Path(item['game']).name
 verify(path,item['sha256']);item['game']=str(path)
for item in corpus['models']:
 path=(args.model or pathlib.Path(item['path'])).resolve()
 verify(path,item['sha256']);item['path']=str(path)
p.write_text(json.dumps(corpus,indent=2)+'\n')
print('Verified and located all frozen inputs; model payload is also hash-checked by the NNUE loader.')
