"""Freeze a larger, grouped corpus for the 512 pilot; reuse the Rust replay exporter."""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile

import numpy as np

MOVE_KEYS = ('color', 'from_file', 'from_rank', 'to_file', 'to_rank', 'promoted', 'data')


def hashed(x):
    return hashlib.sha256(json.dumps(x, sort_keys=True).encode()).hexdigest()


def digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''):
            h.update(chunk)
    return h.hexdigest()


def position_key(data, s):
    start, middle = s['offset'], s['offset'] + s['us']
    # Feature order affects accumulation, but must not defeat duplicate detection.
    return hashlib.sha256(np.sort(data[start:middle]).tobytes() + b'|' +
                          np.sort(data[middle:middle+s['them']]).tobytes()).hexdigest()


def stratified(game, count, salt):
    valid = [i for i, m in enumerate(game['moves']) if i >= 16 and
             type(m.get('eval')) is int and abs(m['eval']) < 900000]
    picked = []
    for color in ('Black', 'White'):
        pool = [i for i in valid if game['moves'][i]['color'] == color]
        n = min(count // 2, len(pool))
        for b in range(n):
            bucket = pool[b*len(pool)//n:(b+1)*len(pool)//n]
            choices = sorted(bucket, key=lambda i: hashed([salt, i]))
            choice = next((i for i in choices if all(abs(i-j) >= 4 for j in picked)), None)
            if choice is not None:
                picked.append(choice)
    return picked


def main():
    p = argparse.ArgumentParser()
    p.add_argument('out', type=Path)
    p.add_argument('--old', type=Path, default=Path('data/nnue-training'))
    p.add_argument('--binary', default='target/release/nnue_tool')
    a = p.parse_args()
    root, old = a.out.resolve(), a.old.resolve()
    if (root/'dataset').exists():
        raise ValueError('Use a fresh output directory')
    with tarfile.open(root/'snapshot.tar.gz') as tar:
        tar.extractall(root/'snapshot', filter='data')
    snap = json.loads((root/'snapshot/snapshot.json').read_text())
    prior = json.loads((old/'manifest.json').read_text())
    labels = {}
    for name in snap['labels']:
        for line in (root/'snapshot'/name).read_text().splitlines():
            m = json.loads(line)
            for search in m['searches']:
                if search.get('completed_depth', 0) >= 1 and abs(search['score']) < 900000:
                    # Analyzer plies are one-based BEFORE-move indices; exporter uses zero-based.
                    labels[m['game_hash'], search['ply']-1] = dict(
                        score=search['score'], selection=m['selection'], depth=search['completed_depth'],
                        teacher=search['model_sha256'], watchdog=search.get('hard_timeout', False))
    records = {}
    for entry in prior['games']:
        path = old/entry['path']
        records[entry['sha256']] = dict(path=str(path), prior_split=entry['split'], old=True)
    for entry in snap['games']:
        records[entry['sha256']] = dict(path=str(root/'snapshot'/entry['path']),
                                       pair=[entry['run'], entry['pair_seed']], old=False)
    parents = {h: h for h in records}

    def find(h):
        while parents[h] != h:
            parents[h] = parents[parents[h]]
            h = parents[h]
        return h

    keys = {}
    for h, r in records.items():
        if digest(r['path']) != h:
            raise ValueError('Game content changed: '+r['path'])
        g = json.loads(Path(r['path']).read_text())
        r['game'] = g
        prefix = [{k: m[k] for k in MOVE_KEYS if k in m} for m in g['moves'][:64]]
        groups = [('prefix', hashed([g['start'], prefix]))]
        if 'pair' in r:
            groups.append(('pair', hashed(r['pair'])))
        for key in groups:
            if key in keys:
                parents[find(h)] = find(keys[key])
            keys[key] = h
    groups = defaultdict(list)
    for h in records:
        groups[find(h)].append(h)
    splits = {}
    for hs in groups.values():
        prior_splits = {records[h].get('prior_split') for h in hs}
        bucket = int(hashed(sorted(hs))[:8], 16) % 10
        split = ('train' if 'train' in prior_splits else 'validation' if 'validation' in prior_splits
                 else 'test' if bucket == 0 else 'validation' if bucket == 1 else 'train')
        for h in hs:
            splits[h] = split
    jobs, meta = [], {}
    for h, r in records.items():
        g = r['game']
        if not g.get('result') or g.get('abort_reason'):
            continue
        selected = {i: 'representative' for i in stratified(g, 24 if r['old'] else 64, h)}
        for (gh, i), label in labels.items():
            if gh == h:
                selected[i] = label['selection']
        if not selected:
            continue
        jobs.append(dict(game=r['path'], plies=sorted(selected),
                         split='train' if splits[h] == 'train' else 'validation'))
        for i, selection in selected.items():
            move = g['moves'][i]
            sign = 1 if move['color'] == 'Black' else -1
            label = labels.get((h, i))
            # Draw records lack termination reasons: do not use any as outcome targets.
            result = g['result']
            outcome = None if result == 'Draw' else float((result == 'BlackWins') == (sign == 1))
            meta[r['path'], i] = dict(split=splits[h], group=find(h), game_sha256=h,
                source='historical' if r['old'] else 'recent', selection=selection,
                label_source='analysis' if label else 'saved', outcome=outcome,
                side=move['color'], game_length=len(g['moves']),
                score_override=label['score']*sign if label else None,
                teacher=label['teacher'] if label else g[move['color'].lower()].get('model'),
                completed_depth=label['depth'] if label else move.get('completed_depth'))
    # Held-out first: exporter duplicate elimination must not preferentially retain training copies.
    jobs.sort(key=lambda j: (j['split'] == 'train', j['game']))
    (root/'jobs.jsonl').write_text(''.join(json.dumps(j)+'\n' for j in jobs))
    subprocess.run([a.binary, 'export', str(old/'base.json'), str(root/'jobs.jsonl'), str(root/'dataset')], check=True)
    ds = root/'dataset'
    samples = [json.loads(x) for x in (ds/'samples.jsonl').read_text().splitlines()]
    data = np.memmap(ds/'features.bin', mode='r', dtype='<u4')
    prior_samples = [json.loads(x) for x in (old/'dataset-v1/samples.jsonl').read_text().splitlines()]
    prior_data = np.memmap(old/'dataset-v1/features.bin', mode='r', dtype='<u4')
    old_train = {position_key(prior_data, s) for s in prior_samples if s['split'] == 'train'}
    seen, kept, dropped = set(), [], Counter()
    for s in samples:
        metadata = meta[s['game'], s['ply']]
        key = position_key(data, s)
        if key in seen or (metadata['split'] != 'train' and key in old_train):
            dropped['duplicate_or_parent_training_overlap'] += 1
            continue
        seen.add(key)
        s.update(metadata)
        override = s.pop('score_override')
        if override is not None:
            s['score'] = override
        s['canonical_position_hash'] = key
        kept.append(s)
    (ds/'samples.jsonl').write_text(''.join(json.dumps(s)+'\n' for s in kept))
    identity = json.loads((ds/'dataset.json').read_text())
    identity.update(samples=len(kept), samples_sha256=digest(ds/'samples.jsonl'),
                    snapshot_sha256=digest(root/'snapshot.tar.gz'), policy='512-pilot-v1')
    (ds/'dataset.json').write_text(json.dumps(identity, indent=2)+'\n')
    report = dict(positions=len(kept), games=len(jobs), splits=Counter(s['split'] for s in kept),
                  sources=Counter(s['source'] for s in kept), labels=Counter(s['label_source'] for s in kept),
                  selections=Counter(s['selection'] for s in kept), dropped=dropped,
                  split_games={sp:len({s['game'] for s in kept if s['split']==sp}) for sp in ('train','validation','test')},
                  rule='Pair seeds and exact 64-move prefixes grouped; parent training positions excluded from held-out sets; draws have no outcome target.')
    (root/'data-report.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
