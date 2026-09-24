"""Preselect held-out starts, verify exported models, and run paired 3s screening games."""
import argparse
from collections import Counter
import json
from pathlib import Path
import subprocess

import numpy as np

from pilot_data import digest, hashed
from quantized import Quantized


def game_path(sample):
    """Relocate frozen local paths without rewriting the corpus identity."""
    path = Path(sample['game'])
    if not path.exists():
        marker = '/taikyoku_shogi/'
        if marker in str(path):
            path = Path(str(path).split(marker, 1)[1])
    if digest(path) != sample['game_sha256']:
        raise ValueError(f'Frozen game contents changed: {path}')
    return str(path.resolve())


def freeze(root):
    samples=[json.loads(x) for x in (root/'dataset/samples.jsonl').read_text().splitlines()]
    test=[s for s in samples if s['split']=='test']
    # Late, approximately balanced, nonterminal teacher-labelled roots permit a bounded
    # screen. This is deliberately not an opening tournament or an Elo measurement.
    eligible=[s for s in test if s['label_source']=='analysis' and abs(s['score'])<2000
              and s['ply']>100 and 40 < s['game_length']-s['ply'] <= 200]
    eligible.sort(key=lambda s:hashed(['pilot-matches-v1',s['game_sha256'],s['ply']]))
    starts=[]; seen=set()
    for s in eligible:
        if s['group'] not in seen:
            starts.append(s); seen.add(s['group'])
        if len(starts)==4:
            break
    if len(starts)!=4:
        raise ValueError('Insufficient independent held-out match starts')
    checks=[];seen=set()
    for s in sorted(test,key=lambda s:hashed(['pilot-checks-v1',s['game_sha256'],s['ply']])):
        if s['group'] not in seen:
            checks.append(s); seen.add(s['group'])
        if len(checks)==16:
            break
    manifest=dict(version=1,starts=starts,checks=checks,time_ms=3000,max_plies=160,
                  policy='Four independent held-out late-game starts; both colors; caps unresolved',
                  dataset_sha256=digest(root/'dataset/dataset.json'))
    path=root/'evaluation-manifest.json'
    if path.exists() and json.loads(path.read_text())!=manifest:
        raise ValueError('Evaluation manifest already frozen differently')
    path.write_text(json.dumps(manifest,indent=2)+'\n')
    print(json.dumps(dict(starts=[dict(game=s['game'],ply=s['ply'],remaining=s['game_length']-s['ply'],score=s['score']) for s in starts])))


def verify(root, model, cpu):
    manifest=json.loads((root/'evaluation-manifest.json').read_text())
    cp=json.loads(model.read_text()); blob=model.parent/cp['weights']['nnue']['file']
    assert digest(blob)==cp['weights']['nnue']['sha256']
    schema=json.loads((root/'dataset/schema.json').read_text())
    q=Quantized(blob,schema['features'])
    data=np.memmap(root/'dataset/features.bin',mode='r',dtype='<u4')
    results=[]
    for s in manifest['checks']:
        begin=s['offset'];middle=begin+s['us'];end=middle+s['them']
        expected=round(s['material'])+q.residual(data[begin:middle],data[middle:end])
        raw=subprocess.run(['taskset','-c',str(cpu),'target/release/nnue_tool','evaluate',str(model),
                            game_path(s),str(s['ply']),'3000'],check=True,capture_output=True,text=True,timeout=60)
        result=json.loads(raw.stdout)
        assert abs(result['score']-expected)<=1,(result['score'],expected)
        assert result['search']['legal'] and result['search']['depth']>=1,result
        results.append(dict(game=s['game'],ply=s['ply'],reference=expected,**result))
    (model.parent/'checks.json').write_text(json.dumps(results,indent=2)+'\n')
    print(json.dumps(dict(model=str(model),checks=len(results))),flush=True)


def matches(root, model, parent, cpu):
    manifest=json.loads((root/'evaluation-manifest.json').read_text())
    out=model.parent/'matches';out.mkdir(exist_ok=True)
    for i,s in enumerate(manifest['starts']):
        for black in (True,False):
            name=f'{i}-'+('black' if black else 'white')
            result=out/(name+'.result.json')
            job=dict(game=game_path(s),ply=s['ply'],candidate=str(model.resolve()),
                     parent=str(parent.resolve()),candidate_black=black,time_ms=manifest['time_ms'],
                     max_plies=manifest['max_plies'],out=str((out/(name+'.game.json')).resolve()),
                     candidate_sha256=digest(model),parent_sha256=digest(parent),
                     binary_sha256=digest('target/release/nnue_pilot_match'))
            path=out/(name+'.job.json')
            if result.exists():
                if json.loads(path.read_text())!=job:
                    raise ValueError('Completed game belongs to different models or settings')
                continue
            path.write_text(json.dumps(job,indent=2)+'\n')
            log=out/(name+'.jsonl')
            with log.open('w') as f, (out/(name+'.stderr')).open('w') as err:
                try:
                    subprocess.run(['taskset','-c',str(cpu),'target/release/nnue_pilot_match',str(path)],
                                   check=True,stdout=f,stderr=err,timeout=manifest['max_plies']*3+120)
                except subprocess.TimeoutExpired:
                    result.write_text(json.dumps(dict(event='watchdog',candidate_score=None))+'\n')
                    continue
            events=[json.loads(x) for x in log.read_text().splitlines()]
            assert events[-1]['event']=='complete'
            result.write_text(json.dumps(events[-1],indent=2)+'\n')
            print(json.dumps(dict(model=model.parent.name,pair=i,black=black,**events[-1])),flush=True)
    results=[json.loads(p.read_text()) for p in sorted(out.glob('*.result.json'))]
    counts=Counter('unresolved' if r['candidate_score'] is None else
                   'win' if r['candidate_score']==1 else 'loss' if r['candidate_score']==0 else 'draw' for r in results)
    (model.parent/'matches-summary.json').write_text(json.dumps(dict(counts=counts,results=results),indent=2)+'\n')


def main():
    p=argparse.ArgumentParser()
    p.add_argument('action',choices=['freeze','verify','matches'])
    p.add_argument('root',type=Path)
    p.add_argument('--model',type=Path)
    p.add_argument('--parent',type=Path,default=Path('data/nnue-admission-v2/NNUE_W512_v2.json'))
    p.add_argument('--cpu',type=int,default=0)
    a=p.parse_args()
    if a.action=='freeze':freeze(a.root)
    elif a.action=='verify':verify(a.root,a.model,a.cpu)
    else:matches(a.root,a.model,a.parent,a.cpu)


if __name__=='__main__':main()
