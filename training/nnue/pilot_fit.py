"""Isolated 512 data/loss pilot. Never modifies production checkpoints or trainer defaults."""
import argparse
from collections import Counter
import json
import math
import os
from pathlib import Path
import random
import time

import numpy as np
import torch

from pilot_data import digest
from plateau import Plateau, Policy
from quantized import Quantized
from train import Net, batch, export, atomic_json


def rounded(value, scale, half_up=False):
    scaled = value*scale
    discrete = (torch.floor(scaled+.5) if half_up else torch.round(scaled))/scale
    return value + (discrete-value).detach()


class PilotNet(Net):
    """Use deployed quantization in the forward pass and floating sparse gradients.

    Only active embedding rows are quantized, avoiding a full-table copy per batch.
    Floating master weights accumulate updates smaller than a quantization step.
    """
    def forward(self, indices, offsets):
        continuous = self.embedding(indices, offsets)+self.bias
        with torch.no_grad():
            unique, inverse = torch.unique(indices,return_inverse=True)
            rows = torch.round(self.embedding.weight.index_select(0,unique)*4096).clamp(-2048,2048)/4096
            discrete = torch.nn.functional.embedding_bag(inverse,rows,offsets,mode='sum',
                                                         include_last_offset=True)
            discrete += torch.round(self.bias*4096)/4096
        x = continuous+(discrete-continuous).detach()
        x = x.clamp(0,1).reshape(-1,self.width*2)
        x = rounded(x,127,half_up=True)
        for layer in (self.h1,self.h2):
            w = rounded(layer.weight,64)
            if layer is self.h1:
                # Export folds the centering offset into the integer bias.
                shift = w.sum(dim=1)*.5
                bias = rounded(layer.bias-shift,127*64)+shift
                x = x-.5
            else:
                bias = rounded(layer.bias,127*64)
            x = torch.nn.functional.linear(x,w,bias).clamp(0,1)
            x = rounded(x,127,half_up=True)
        return self.output(x).squeeze(-1)


def from_quantized(path, features):
    q = Quantized(path, features)
    net = PilotNet(features, q.width)
    with torch.no_grad():
        for first in range(0, features, 1024):
            net.embedding.weight[first:first+1024].copy_(torch.from_numpy(
                q.weights[first:first+1024].astype(np.float32)/4096))
        net.bias.copy_(torch.from_numpy(q.bias.astype(np.float32)/4096))
        for layer, w, b in [(net.h1, q.h1, q.b1), (net.h2, q.h2, q.b2)]:
            layer.weight.copy_(torch.from_numpy(w.astype(np.float32)/64))
            bias = b.astype(np.float64)
            if layer is net.h1:
                bias = bias + w.astype(np.int32).sum(axis=1)*63.5
            layer.bias.copy_(torch.from_numpy((bias/(127*64)).astype(np.float32)))
        net.output.weight.copy_(torch.from_numpy(q.out.copy()).reshape(1, -1))
        net.output.bias.fill_(float(q.out_bias))
    return net


def calibration(samples):
    """Single symmetric temperature, fit on training outcomes only, equal game weight."""
    rows = [s for s in samples if s['split']=='train' and s['outcome'] is not None]
    counts = Counter(s['game'] for s in rows)
    weights = np.array([1/counts[s['game']] for s in rows])
    score = np.array([s['score'] for s in rows])
    result = np.array([s['outcome'] for s in rows])
    candidates = np.geomspace(100, 50000, 161)
    losses = [float(np.average((1/(1+np.exp(-np.clip(score/t, -40, 40)))-result)**2,
                              weights=weights)) for t in candidates]
    i = int(np.argmin(losses))
    if i in (0, len(candidates)-1):
        raise ValueError('Calibration optimum at boundary; inspect before training')
    return dict(scale=float(candidates[i]), training_brier=losses[i],
                games=len(counts), positions=len(rows), draw_policy='excluded: termination reason unavailable')


def category(s):
    if s['selection']!='representative':
        return 'targeted'
    if s['label_source']=='analysis' or 'NNUE' in str(s['teacher']):
        return 'nnue'
    return 'handcrafted'


def weights_for(samples, ids):
    # Teacher quality, not game date: recent tournaments still contain many HCE scores.
    proportions = dict(handcrafted=.2, nnue=.6, targeted=.2)
    counts = Counter(category(samples[i]) for i in ids)
    normalizer = sum(proportions[k] for k in counts)
    return {i:proportions[category(samples[i])]/normalizer*len(ids)/counts[category(samples[i])]
            for i in ids}


def losses(pred, target, material, outcome, mode, scale, mix):
    huber = torch.nn.functional.smooth_l1_loss(pred, target, reduction='none')
    total = pred*1000 + material
    teacher = target*1000 + material
    p, q = torch.sigmoid(total/scale), torch.sigmoid(teacher/scale)
    mask = torch.isfinite(outcome)
    blended = torch.where(mask, (1-mix)*q+mix*torch.nan_to_num(outcome), q)
    # Match the Huber curvature near equality at score zero; preserve native engine score units.
    loss = huber if mode=='huber' else (p-blended).square() * (8*(scale/1000)**2)
    return loss, huber, p, q, mask


def evaluate(net, samples, data, ids, batch_size, mode, scale, mix):
    weights = weights_for(samples, ids)
    totals = Counter()
    by_source = {k:Counter() for k in ('historical','recent','analysis')}
    with torch.no_grad():
        for start in range(0, len(ids), batch_size):
            group = ids[start:start+batch_size]
            x, off, target = batch(samples, data, group)
            material = torch.tensor([samples[i]['material'] for i in group])
            outcome = torch.tensor([float('nan') if samples[i]['outcome'] is None else samples[i]['outcome'] for i in group])
            pred = net(x, off)
            loss, huber, probability, teacher, mask = losses(pred, target, material, outcome, mode, scale, mix)
            w = torch.tensor([weights[i] for i in group])
            totals['objective'] += (loss*w).sum().item()
            totals['huber'] += huber.sum().item()
            totals['mae'] += (pred-target).abs().sum().item()*1000
            totals['teacher_brier'] += (probability-teacher).square().sum().item()
            totals['outcome_brier'] += (probability[mask]-outcome[mask]).square().sum().item()
            totals['outcome_count'] += mask.sum().item()
            for j, i in enumerate(group):
                key = 'analysis' if samples[i]['label_source']=='analysis' else samples[i]['source']
                by_source[key]['count'] += 1
                by_source[key]['huber'] += huber[j].item()
    result = {k: v/(totals['outcome_count'] if k=='outcome_brier' else len(ids))
              for k,v in totals.items() if k!='outcome_count'}
    result.update(count=len(ids), outcome_count=totals['outcome_count'],
                  by_source={k:dict(count=v['count'],huber=v['huber']/v['count']) for k,v in by_source.items() if v['count']})
    return result


def main():
    p = argparse.ArgumentParser()
    p.add_argument('dataset', type=Path)
    p.add_argument('--parent', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--init', choices=['fresh','parent'], required=True)
    p.add_argument('--loss', choices=['huber','wdl'], required=True)
    p.add_argument('--mix', type=float, default=0)
    p.add_argument('--epochs', type=int, default=12)
    p.add_argument('--batch', type=int, default=8)
    p.add_argument('--epoch-samples', type=int, default=32768,
                   help='Weighted draws per pass; all arms use the same deterministic draws')
    p.add_argument('--seed', type=int, default=20260924)
    p.add_argument('--resume', action='store_true')
    p.add_argument('--limit', type=int)
    a = p.parse_args()
    if not 0 <= a.mix <= 1 or (a.loss=='huber' and a.mix):
        raise ValueError('Invalid mixture')
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.manual_seed(a.seed)
    np.random.seed(a.seed)
    schema = json.loads((a.dataset/'schema.json').read_text())
    identity = json.loads((a.dataset/'dataset.json').read_text())
    for key, suffix in [('features','bin'),('samples','jsonl'),('schema','json')]:
        assert digest(a.dataset/f'{key}.{suffix}') == identity[key+'_sha256']
    samples = [json.loads(x) for x in (a.dataset/'samples.jsonl').read_text().splitlines()]
    data = np.memmap(a.dataset/'features.bin', mode='r', dtype='<u4')
    train = [i for i,s in enumerate(samples) if s['split']=='train']
    val = [i for i,s in enumerate(samples) if s['split']=='validation']
    test = [i for i,s in enumerate(samples) if s['split']=='test']
    if a.limit:
        train, val, test = train[:a.limit], val[:max(16,a.limit//10)], test[:max(16,a.limit//10)]
    assert train and val and test
    assert not ({samples[i]['group'] for i in train} & {samples[i]['group'] for i in val+test})
    cal = calibration(samples)
    parent = json.loads(a.parent.read_text())
    desc = parent['weights']['nnue']
    blob = (a.parent.parent/desc['file']).resolve()
    assert digest(blob)==desc['sha256'] and desc['width']==512 and desc['feature_hash']==schema['names_hash']
    recipe = dict(dataset=identity, parent_sha256=digest(a.parent), init=a.init, loss=a.loss,
                  mix=a.mix, batch=a.batch, seed=a.seed, epochs=a.epochs, limit=a.limit,
                  epoch_samples=a.epoch_samples,
                  code_sha256=digest(__file__), scale=cal['scale'],
                  quantization='active-feature-and-head-aware; sparse floating master weights')
    a.out.mkdir(parents=True, exist_ok=True)
    if (a.out/'recipe.json').exists():
        assert a.resume and json.loads((a.out/'recipe.json').read_text())==recipe
    else:
        atomic_json(a.out/'recipe.json', recipe)
    atomic_json(a.out/'calibration.json', cal)
    policy = Policy(min_epochs=4, max_epochs=a.epochs, patience=2, lr_reductions=1)
    first, best, metrics = 0, math.inf, []
    state = None
    if a.resume and (a.out/'training.pt').exists():
        state = torch.load(a.out/'training.pt', weights_only=True, mmap=True)
        with torch.device('meta'):
            net = PilotNet(schema['features'], 512)
        net.load_state_dict(state['net'], assign=True)
        first, best, metrics = state['epoch'], state['best'], state['metrics']
        tracker = Plateau(**state['plateau'])
    else:
        net = from_quantized(blob, schema['features']) if a.init=='parent' else PilotNet(schema['features'],512)
        initial = evaluate(net,samples,data,val,a.batch,a.loss,cal['scale'],a.mix)
        metrics.append(dict(epoch=0, validation=initial))
        tracker = Plateau(initial['objective'])
        print(json.dumps(metrics[-1]), flush=True)
    emb = torch.optim.SGD([net.embedding.weight], lr=.0002)
    dense = torch.optim.Adam([v for k,v in net.named_parameters() if k!='embedding.weight'], lr=.0003)
    if state:
        emb.load_state_dict(state['emb']); dense.load_state_dict(state['dense']); del state
    weights = weights_for(samples, train)
    started = time.monotonic()
    for epoch in range(first, a.epochs):
        if tracker.stopped:
            break
        order = random.Random(a.seed+epoch).choices(train, weights=[weights[i] for i in train],
                                                  k=min(a.epoch_samples, len(train)) if a.limit else a.epoch_samples)
        total = 0
        for start in range(0, len(order), a.batch):
            ids = order[start:start+a.batch]
            x, off, target = batch(samples,data,ids)
            material = torch.tensor([samples[i]['material'] for i in ids])
            outcome = torch.tensor([float('nan') if samples[i]['outcome'] is None else samples[i]['outcome'] for i in ids])
            emb.zero_grad(set_to_none=True); dense.zero_grad(set_to_none=True)
            pred = net(x,off)
            loss = losses(pred,target,material,outcome,a.loss,cal['scale'],a.mix)[0]
            loss = loss.mean()  # Sampling already supplies the category weights.
            assert torch.isfinite(loss)
            loss.backward(); emb.step(); dense.step()
            with torch.no_grad():
                touched = torch.unique(x)
                net.embedding.weight[touched] = net.embedding.weight[touched].clamp(-.5,.5)
                net.bias.clamp_(-2,2)
                for layer in [net.h1,net.h2]:
                    layer.weight.clamp_(-127/64,127/64); layer.bias.clamp_(-8,8)
            total += loss.item()*len(ids)
            if start % (a.batch*100)==0:
                atomic_json(a.out/'status.json',dict(state='training',epoch=epoch+1,samples=start+len(ids),
                    total=len(order),elapsed_s=time.monotonic()-started))
        validation = evaluate(net,samples,data,val,a.batch,a.loss,cal['scale'],a.mix)
        record = dict(epoch=epoch+1, train_objective=total/len(order), validation=validation,
                      elapsed_s=time.monotonic()-started)
        if validation['objective'] < best:
            best = validation['objective']
            candidate = a.out/'candidate.bin'
            sha = export(net,candidate,schema)
            q = Quantized(candidate,schema['features'])
            errors=[]
            with torch.no_grad():
                for i in val[::max(1,len(val)//64)][:64]:
                    s=samples[i]; begin=s['offset']; middle=begin+s['us']
                    x,off,_=batch(samples,data,[i])
                    errors.append(abs(q.residual(data[begin:middle],data[middle:middle+s['them']])-net(x,off).item()*1000))
            del q
            assert max(errors)<250 and np.mean(errors)<50, errors
            record['quantization'] = dict(mean=float(np.mean(errors)),max=float(max(errors)))
            published = a.out/f'nnue-{sha}.bin'; candidate.replace(published)
            cp = json.loads(a.parent.read_text()); cp['name']=a.out.name
            cp['weights']['nnue'].update(file=published.name,sha256=sha)
            old_blob = None
            if (a.out/'model.json').exists():
                old_blob = a.out/json.loads((a.out/'model.json').read_text())['weights']['nnue']['file']
            atomic_json(a.out/'model.json',cp)
            if old_blob and old_blob!=published:
                old_blob.unlink()
        decision = tracker.observe(epoch+1,validation['objective'],policy)
        if decision=='reduce_lr':
            for optimizer in [emb,dense]:
                for group in optimizer.param_groups:
                    group['lr']*=policy.lr_factor
        record['decision']=decision; metrics.append(record)
        atomic_json(a.out/'metrics.json',metrics)
        tmp=a.out/'training.tmp'
        torch.save(dict(net=net.state_dict(),emb=emb.state_dict(),dense=dense.state_dict(),
                        epoch=epoch+1,best=best,metrics=metrics,plateau=tracker.__dict__),tmp)
        tmp.replace(a.out/'training.pt')
        print(json.dumps(record),flush=True)
    del net,emb,dense
    cp=json.loads((a.out/'model.json').read_text())
    best_net=from_quantized(a.out/cp['weights']['nnue']['file'],schema['features'])
    # Untouched test set is evaluated once after all checkpoint selection is complete.
    test_result=evaluate(best_net,samples,data,test,a.batch,a.loss,cal['scale'],a.mix)
    atomic_json(a.out/'test.json',test_result)
    atomic_json(a.out/'status.json',dict(state='completed',reason=tracker.stopped,test=test_result))
    print(json.dumps(dict(test=test_result)),flush=True)


if __name__=='__main__':
    main()
