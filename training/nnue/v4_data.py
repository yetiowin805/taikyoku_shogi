"""Frozen v4 corpus: reuse old features/labels; no new searches or live mutations."""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
import math
from pathlib import Path
import sqlite3
import subprocess

import numpy as np
from pilot_data import digest, hashed, position_key, stratified, MOVE_KEYS

SCALE = 2822.9057434005167


def probability(score):
    if abs(score) >= 900000:
        return float(score > 0)
    return 1 / (1 + math.exp(-max(-40, min(40, score / SCALE))))


def combine(searches):
    """One vote per immutable teacher identity; only completed search results."""
    by_teacher = {}
    for s in searches:
        if s.get('completed_depth', 0) < 1 or not math.isfinite(s.get('score', float('nan'))):
            continue
        teacher = s.get('model_sha256')
        if not teacher:
            raise ValueError('Teacher lacks content identity')
        if teacher not in by_teacher or s['completed_depth'] > by_teacher[teacher]['completed_depth']:
            by_teacher[teacher] = s
    if not by_teacher:
        return None
    q = sum(probability(s['score']) for s in by_teacher.values()) / len(by_teacher)
    clipped = min(1-1e-6, max(1e-6, q))
    return dict(score=SCALE*math.log(clipped/(1-clipped)), target_probability=q,
                mate=any(abs(s['score'])>=900000 for s in by_teacher.values()),
                teachers=sorted(by_teacher), completed_depth=min(s['completed_depth'] for s in by_teacher.values()))


class Features:
    """Virtual concatenation, so the immutable 1.7GB parent file need not be copied."""
    def __init__(self, root):
        identity = json.loads((Path(root)/'dataset.json').read_text())
        self.shards=[];start=0
        for entry in identity['feature_shards']:
            p=Path(entry['path'])
            if digest(p)!=entry['sha256']:raise ValueError('Feature shard changed: '+str(p))
            values=np.memmap(p,mode='r',dtype='<u4')
            self.shards.append((start,start+len(values),values));start+=len(values)
    def __getitem__(self, key):
        for first,last,values in self.shards:
            if first<=key.start and key.stop<=last:return values[key.start-first:key.stop-first]
        raise IndexError('Feature slice spans shards')


def bucket(s):
    if s.get('mate'):return 'mate'
    if s['source']=='historical':return 'historical'
    return 'targeted' if s['selection']!='representative' else 'recent'


def set_weights(samples):
    # Equal games inside each stratum; long games and multiple teachers cannot
    # acquire extra influence merely by yielding more rows.
    proportions={'historical':.20,'recent':.55,'targeted':.20,'mate':.05}
    for split in ('train','validation','test'):
        rows=[s for s in samples if s['split']==split]
        games=defaultdict(Counter)
        for s in rows:games[bucket(s)][s['game_sha256']]+=1
        normalizer=sum(proportions[k] for k in games)
        for s in rows:
            k=bucket(s);s['sampling_weight']=proportions[k]/normalizer/len(games[k])/games[k][s['game_sha256']]


def build(args):
    out=args.out.resolve();out.mkdir(parents=True,exist_ok=True)
    if (out/'dataset.json').exists():return
    old=args.old.resolve();identity=json.loads((old/'dataset.json').read_text())
    for name,ext in [('samples','jsonl'),('features','bin'),('schema','json')]:
        if digest(old/f'{name}.{ext}')!=identity[name+'_sha256']:raise ValueError('Parent dataset changed')
    samples=[json.loads(x) for x in (old/'samples.jsonl').read_text().splitlines()]
    data=np.memmap(old/'features.bin',mode='r',dtype='<u4')
    old_keys={s['canonical_position_hash']:s for s in samples}
    old_locations={(s['game_sha256'],s['ply']):s for s in samples}
    records={};old_meta={}
    for s in samples:old_meta[s['game_sha256']]=(s['group'],s['split'],s['game'])
    by_filename=defaultdict(list)
    for p in args.games.glob('*/slot*.json'):by_filename[p.name].append(p)
    for h,(group,split,path) in old_meta.items():
        candidates=by_filename.get(Path(path).name,[])
        p=next((p for p in candidates if digest(p)==h),None)
        if p:records[h]=dict(path=str(p),group=group,split=split,old=True)
        else:raise ValueError('Missing unchanged parent game: '+path)
    labels=defaultdict(list);selections={}
    for dbpath in args.catalogue:
        db=sqlite3.connect('file:'+str(dbpath.resolve())+'?mode=ro',uri=True)
        for (payload,) in db.execute('select payload from moments where status=?',('completed',)):
            m=json.loads(payload)
            for search in m.get('searches',[]):
                key=(m['game_hash'],search['ply']-1)
                labels[key].append(search);selections[key]=m.get('selection','representative')
        db.close()
    (out/'teacher-labels.jsonl').write_text(''.join(json.dumps(dict(game=h,ply=i,searches=ss))+'\n' for (h,i),ss in sorted(labels.items())))
    labels={k:v for k,s in labels.items() if (v:=combine(s)) is not None}
    for run in args.runs:
        state=json.loads((run/'state.json').read_text())
        for slot in state['slots']:
            if slot['status']!='done' or not slot.get('game_path'):continue
            p=Path(slot['game_path'])
            if not p.is_absolute():p=args.repo/p
            h=digest(p)
            if h not in records:records[h]=dict(path=str(p),old=False)
            records[h]['pair']=[str(run),slot['start_seed']]
    (out/'games.json').write_text(json.dumps({h:r['path'] for h,r in records.items()},indent=2)+'\n')
    # Freeze grouping before any feature export; inherited parent groups/splits
    # dominate new assignments. Conflicting prior split groups are never merged
    # into training; new members of such groups are quarantined.
    parents={h:h for h in records}
    def find(h):
        while parents[h]!=h:parents[h]=parents[parents[h]];h=parents[h]
        return h
    seen={}
    for h,r in records.items():
        g=json.loads(Path(r['path']).read_text());r['game']=g
        prefix=[{k:m[k] for k in MOVE_KEYS if k in m} for m in g['moves'][:64]]
        keys=[('prefix',hashed([g['start'],prefix]))]
        if 'group' in r:keys.append(('parent',r['group']))
        if 'pair' in r:keys.append(('pair',hashed(r['pair'])))
        for key in keys:
            if key in seen:parents[find(h)]=find(seen[key])
            seen[key]=h
    groups=defaultdict(list)
    for h in records:groups[find(h)].append(h)
    dropped=Counter()
    for hs in groups.values():
        splits={records[h]['split'] for h in hs if 'split' in records[h]}
        inherited={records[h]['group'] for h in hs if 'group' in records[h]}
        canonical=min(inherited) if inherited else min(hs)
        b=int(hashed(sorted(hs))[:8],16)%10
        split=next(iter(splits)) if len(splits)==1 else 'test' if b==0 else 'validation' if b==1 else 'train'
        for h in hs:
            records[h]['quarantine']=len(splits)>1
            records[h]['assigned_group']=canonical
            records[h]['assigned_split']=split
    jobs=[];metadata={}
    for h,r in records.items():
        g=r['game']
        if r['quarantine']:
            dropped['conflicting_parent_group']+=1;continue
        selected={} if r['old'] else {i:'representative' for i in stratified(g,32,h)}
        for (gh,i),label in labels.items():
            if gh==h:selected[i]=selections[gh,i]
        if not g.get('result') or g.get('abort_reason'):continue
        pending=[];scores={}
        for i,selection in sorted(selected.items()):
            if not 0<=i<len(g['moves']):continue
            move=g['moves'][i];label=labels.get((h,i));sign=1 if move['color']=='Black' else -1
            if label is None and (type(move.get('eval'))is not int or abs(move['eval'])>=900000):continue
            original=old_locations.get((h,i))
            # Preserve held-out historical labels as an independent unchanged regression set.
            if original and original['split']!='train':continue
            score=(label['score'] if label else move['eval'])*sign
            meta=dict(group=r['assigned_group'],split=r['assigned_split'],game_sha256=h,
                      source=original['source'] if original else 'recent',selection=selection,
                      label_source='analysis' if label else 'saved',score=score,
                      mate=bool(label and label['mate']),teacher=label['teachers'] if label else g[move['color'].lower()].get('model'),
                      completed_depth=label['completed_depth'] if label else move.get('completed_depth'),
                      outcome=None if g['result']=='Draw' else float((g['result']=='BlackWins')==(sign==1)),
                      side=move['color'],game_length=len(g['moves']))
            if original:
                original.update({k:v for k,v in meta.items() if k not in ('split','group')});continue
            pending.append(i);metadata[r['path'],i]=meta;scores[str(i)]=score*sign
        if pending:jobs.append(dict(game=r['path'],plies=pending,scores=scores,split='train' if r['assigned_split']=='train' else 'validation'))
    # Remove duplicate inherited features (normally already unique), and order
    # held-out jobs first so new duplicates cannot preferentially become train.
    jobs.sort(key=lambda j:(j['split']=='train',j['game']))
    (out/'jobs.jsonl').write_text(''.join(json.dumps(j)+'\n' for j in jobs))
    fresh=out/'new-features'
    if fresh.exists():
        import shutil
        shutil.rmtree(fresh) # only this run's incomplete derived export
    subprocess.run([str(args.binary),'export',str(args.base),str(out/'jobs.jsonl'),str(fresh)],check=True)
    new_data=np.memmap(fresh/'features.bin',mode='r',dtype='<u4')
    for line in (fresh/'samples.jsonl').read_text().splitlines():
        s=json.loads(line);meta=metadata[s['game'],s['ply']];key=position_key(new_data,s)
        if key in old_keys:dropped['duplicate_position']+=1;continue
        s.update(meta);s['canonical_position_hash']=key;old_keys[key]=s
        s['offset']+=len(data);samples.append(s)
    # Parent samples keep their group IDs; attach new group IDs consistently.
    for s in samples:
        r=records[s['game_sha256']]
        s['game']=r['path']
        if not r['quarantine']:s['group']=r['assigned_group']
    set_weights(samples)
    for split in ('validation','test'):
        if {s['group'] for s in samples if s['split']=='train'} & {s['group'] for s in samples if s['split']==split}:
            raise ValueError('Parent grouping overlaps held-out data')
    (out/'schema.json').write_bytes((old/'schema.json').read_bytes())
    (out/'samples.jsonl').write_text(''.join(json.dumps(s)+'\n' for s in samples))
    shards=[dict(path=str(p),sha256=digest(p)) for p in (old/'features.bin',fresh/'features.bin')]
    result=dict(version=2,policy='512-v4-v1',scale=SCALE,samples=len(samples),feature_shards=shards,
                samples_sha256=digest(out/'samples.jsonl'),schema_sha256=digest(out/'schema.json'),
                baseline_sha256=identity['baseline_sha256'],parent_dataset_sha256=digest(old/'dataset.json'),
                labels_sha256=digest(out/'teacher-labels.jsonl'),games_sha256=digest(out/'games.json'),jobs_sha256=digest(out/'jobs.jsonl'))
    (out/'dataset.json').write_text(json.dumps(result,indent=2)+'\n')
    report=dict(samples=len(samples),splits=Counter(s['split'] for s in samples),buckets=Counter(bucket(s) for s in samples),
                labels=Counter(s['label_source'] for s in samples),dropped=dropped,
                games=len({s['game_sha256'] for s in samples}),new_searches=0)
    (out/'report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report),flush=True)


def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('out','old','games','repo','binary','base'):p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--runs',type=Path,nargs='+',required=True)
    p.add_argument('--catalogue',type=Path,nargs='+',required=True)
    build(p.parse_args())

if __name__=='__main__':main()
