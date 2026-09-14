#!/usr/bin/env python3
"""Freeze equal samples from eight BASE agents and their original checkpoints."""
import argparse, collections, hashlib, json, random
from pathlib import Path

PREFIX='BASE_P120H50B75_'

def digest(data):return hashlib.sha256(data).hexdigest()

def prepare(repo,run,out,per_agent=100,seed=20260914):
    out.mkdir(parents=True,exist_ok=False);(out/'inputs').mkdir()
    state_bytes=(run/'state.json').read_bytes();state=json.loads(state_bytes)
    config=json.loads((run/'analysis/config.json').read_text())
    agents=sorted(e['id'] for e in state['entrants'] if e['id'].startswith(PREFIX))
    expected={PREFIX+x for x in ['C2','C2S2','C2L','C2A','C2LA','C2S2L','C2S2A','C2S2LA']}
    if set(agents)!=expected:raise ValueError('expected exactly the original eight BASE agents')
    rng=random.Random(seed);cases=[];identities={};records={}
    def freeze(path):
        data=path.read_bytes();sha=digest(data);rel='inputs/'+sha+'.json'
        target=out/rel
        if not target.exists():target.write_bytes(data)
        identities[rel]=sha
        return rel,sha,data
    for agent in agents:
        games=[s for s in state['slots'] if s['status']=='done' and agent in (s['model_a'],s['model_b']) and s.get('game_path')]
        rng.shuffle(games);buckets=[[],[],[]]
        for slot in games[:20]:
            path=Path(slot['game_path']);path=path if path.is_absolute() else repo/path
            game_rel,game_hash,data=freeze(path);game=json.loads(data)
            side='black' if (slot['model_a']==agent)==slot['a_is_black'] else 'white'
            original=game[side]
            if original['name']!='ab' or original.get('engine'):raise ValueError('unsupported original agent')
            model_path=Path(original['model']);model_path=model_path if model_path.is_absolute() else repo/model_path
            if model_path.stem!=agent:raise ValueError('slot and record agent disagree')
            model=config['models'][str(model_path.resolve())]
            model_rel,sha,_=freeze(Path(model['snapshot']))
            if sha!=model['sha256']:raise ValueError('model snapshot identity mismatch')
            moves=game['moves'];eligible=[i for i,m in enumerate(moves) if m['color'].lower()==side]
            # One-based move N is searched after applying N-1 recorded moves.
            for i in eligible:
                bucket=min(2,3*i//max(1,len(moves)))
                buckets[bucket].append(dict(name=f"{agent}-slot{slot['id']}-before{i+1}",agent=agent,
                    group=['early','middle','late'][bucket],game=game_rel,game_sha256=game_hash,
                    ply=i,model=model_rel,model_sha256=sha,slot_id=slot['id'],original_game=str(path),original_side=side))
        selected=[]
        for bucket in buckets:rng.shuffle(bucket)
        seen=set()
        while len(selected)<per_agent:
            progressed=False
            for bucket in buckets:
                while bucket:
                    case=bucket.pop();key=(case['game_sha256'],case['ply'])
                    if key not in seen:
                        seen.add(key);selected.append(case);progressed=True;break
                if len(selected)==per_agent:break
            if not progressed:raise ValueError(f'insufficient distinct positions for {agent}')
        cases.extend(selected)
    (out/'state.json').write_bytes(state_bytes)
    (out/'corpus.json').write_text(json.dumps(cases,indent=2)+'\n')
    manifest=dict(seed=seed,per_agent=per_agent,agents=agents,cases=len(cases),state_sha256=digest(state_bytes),
        run_id=state['run_id'],inputs=identities,selection='20 shuffled completed games per agent; equal early/middle/late ply strata; original side and checkpoint')
    (out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    return manifest

def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ['repo','run','out']:p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--per-agent',type=int,default=100);p.add_argument('--seed',type=int,default=20260914)
    a=p.parse_args();print(json.dumps(prepare(a.repo.resolve(),a.run.resolve(),a.out.resolve(),a.per_agent,a.seed),indent=2))

if __name__=='__main__':main()
