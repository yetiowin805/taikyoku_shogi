"""Frozen v4.5 weights: 90/8/2 strata and bounded parent-error sampling.

No label changes. Difficulty applies only inside training game/episode cells;
validation/test use fixed weights. Feature shards remain shared and immutable.
"""
import argparse
from collections import Counter, defaultdict
import json
import os
from pathlib import Path
import shutil
import time

import torch
from pilot_data import digest
from pilot_fit import from_quantized
from train import batch, atomic_json
from v4_data import Features

PROPORTIONS = {'representative': .90, 'precursor': .08, 'mate': .02}


def multiplier(error):
    return 1. if error < .10 else 1.5 if error <= .25 else 2.


def reweight(samples):
    totals = Counter()
    for s in samples:
        totals[s['split'], s['stratum']] += s['sampling_weight']
    for s in samples:
        norm = sum(p for k, p in PROPORTIONS.items() if (s['split'], k) in totals)
        s['sampling_weight'] *= PROPORTIONS[s['stratum']] / norm / totals[s['split'], s['stratum']]
    cells = defaultdict(list)
    for s in samples:
        if s['split'] == 'train':
            cells[s['stratum'], s['group'], s['game'], s['episode']].append(s)
    for rows in cells.values():
        original = sum(s['sampling_weight'] for s in rows)
        boosted = sum(s['sampling_weight'] * s['difficulty_multiplier'] for s in rows)
        for s in rows:
            s['sampling_weight'] *= s['difficulty_multiplier'] * original / boosted
    return samples


def main():
    p = argparse.ArgumentParser()
    for name in ('source', 'parent', 'out'):
        p.add_argument('--'+name, type=Path, required=True)
    p.add_argument('--cpu-file', type=Path)
    a = p.parse_args()
    torch.set_num_threads(min(2, len(os.sched_getaffinity(0)))); torch.set_num_interop_threads(1)
    identity = json.loads((a.source/'dataset.json').read_text())
    schema = json.loads((a.source/'schema.json').read_text())
    cp = json.loads(a.parent.read_text()); desc = cp['weights']['nnue']
    blob = a.parent.parent/desc['file']
    assert digest(blob) == desc['sha256'] and desc['feature_hash'] == schema['names_hash']
    binding = dict(source_sha256=digest(a.source/'dataset.json'), parent_sha256=digest(a.parent),
                   parent_weights_sha256=desc['sha256'], code_sha256=digest(__file__), proportions=PROPORTIONS)
    a.out.mkdir(parents=True, exist_ok=True)
    if (a.out/'binding.json').exists():
        assert json.loads((a.out/'binding.json').read_text()) == binding
        if (a.out/'dataset.json').exists():
            d = json.loads((a.out/'dataset.json').read_text())
            assert digest(a.out/'samples.jsonl') == d['samples_sha256']
            print('Reusing completed v4.5 preparation', flush=True); return
    atomic_json(a.out/'binding.json', binding)
    assert digest(a.source/'samples.jsonl') == identity['samples_sha256']
    assert digest(a.source/'schema.json') == identity['schema_sha256']
    samples = [json.loads(line) for line in (a.source/'samples.jsonl').open()]
    data = Features(a.source)
    net = from_quantized(blob, schema['features']); net.eval()
    ids = [i for i,s in enumerate(samples) if s['split'] == 'train']
    cache = a.out/'difficulty.jsonl'
    scores = {}
    if cache.exists():
        for line in cache.open():
            try: row = json.loads(line)
            except json.JSONDecodeError: continue  # interrupted final write
            scores[row['index']] = row
        # Repair a partial tail before appending further complete records.
        cache.write_text(''.join(json.dumps(v)+'\n' for v in scores.values()))
    missing = [i for i in ids if i not in scores]
    started = time.monotonic()
    with cache.open('a') as f, torch.no_grad():
        for start in range(0, len(missing), 8):
            if a.cpu_file and start % 800 == 0:
                lease = json.loads(a.cpu_file.read_text())
                cpus = set(lease['cpus'])
                if not cpus:
                    raise RuntimeError('Preparation CPU lease is empty')
                for task in Path('/proc/self/task').iterdir():
                    try: os.sched_setaffinity(int(task.name), cpus)
                    except ProcessLookupError: pass
                torch.set_num_threads(min(2, len(cpus)))
            group = missing[start:start+8]
            x, off, _ = batch(samples, data, group)
            material = torch.tensor([samples[i]['material'] for i in group])
            probs = torch.sigmoid((net(x,off)*1000+material)/identity['scale']).tolist()
            for i, prob in zip(group, probs):
                error = abs(prob-samples[i]['target_probability'])
                row = dict(index=i, probability=prob, error=error, multiplier=multiplier(error))
                scores[i] = row; f.write(json.dumps(row)+'\n')
            if start % 800 == 0:
                f.flush()
                atomic_json(a.out/'status.json', dict(state='scoring',done=len(scores),total=len(ids),elapsed_s=time.monotonic()-started))
    for i in ids:
        samples[i]['difficulty_multiplier'] = scores[i]['multiplier']
        samples[i]['parent_probability_error'] = scores[i]['error']
    reweight(samples)
    with (a.out/'samples.jsonl').open('w') as f:
        for s in samples: f.write(json.dumps(s)+'\n')
    shutil.copy2(a.source/'schema.json', a.out/'schema.json')
    identity.update(policy='v4.5-recent-hard-v1', samples_sha256=digest(a.out/'samples.jsonl'),
                    weighting=binding, source_dataset=str(a.source))
    atomic_json(a.out/'dataset.json', identity)
    atomic_json(a.out/'status.json', dict(state='completed',samples=len(samples),
        difficulty_counts=dict(Counter(str(scores[i]['multiplier']) for i in ids))))
    print(json.dumps(json.loads((a.out/'status.json').read_text())), flush=True)


if __name__ == '__main__': main()
