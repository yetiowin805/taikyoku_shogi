"""Opt-in, paired CPU bookkeeping benchmark; never changes datasets or models.

Each variant runs in a fresh process on one pinned CPU. The baseline imports the
trainer files from the specified git revision. This is not a training-quality or
GPU benchmark. Time profiling separately from these normal wall-clock runs.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import random
import resource
import statistics
import subprocess
import sys
import tarfile
import tempfile
import time


REPO = Path(__file__).resolve().parents[2]


def worker(a):
    os.sched_setaffinity(0, {a.cpu})
    sys.path.insert(0, str(a.module_dir))
    import numpy as np
    import torch
    import pilot_fit as fit
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.manual_seed(a.seed)
    samples = [json.loads(s) for s in (a.dataset/'samples.jsonl').read_text().splitlines()]
    data = np.memmap(a.dataset/'features.bin', mode='r', dtype='<u4')
    schema = json.loads((a.dataset/'schema.json').read_text())
    train = [i for i,s in enumerate(samples) if s['split'] == 'train']
    val = [i for i,s in enumerate(samples) if s['split'] == 'validation']
    weights = fit.weights_for(samples, train)
    rng = random.Random(a.seed)
    order = rng.choices(train, weights=[weights[i] for i in train], k=(a.steps+1)*8)
    validation = rng.sample(val, min(a.validation_samples, len(val)))
    net = fit.PilotNet(schema['features'], a.width)
    emb = torch.optim.SGD([net.embedding.weight], lr=.0002)
    dense = torch.optim.Adam([v for k,v in net.named_parameters() if k != 'embedding.weight'], lr=.0003)
    scale = fit.calibration(samples)['scale']

    def step(ids):
        emb.zero_grad(set_to_none=True)
        dense.zero_grad(set_to_none=True)
        value, touched = fit.backward_batch(net, samples, data, ids, 1, 'wdl', scale, 0)
        emb.step()
        dense.step()
        with torch.no_grad():
            net.embedding.weight[touched] = net.embedding.weight[touched].clamp(-.5, .5)
            net.bias.clamp_(-2, 2)
            for layer in [net.h1, net.h2]:
                layer.weight.clamp_(-127/64, 127/64)
                layer.bias.clamp_(-8, 8)
        return value

    step(order[:8])
    start = time.perf_counter()
    losses = [step(order[i:i+8]) for i in range(8, len(order), 8)]
    train_s = time.perf_counter()-start
    start = time.perf_counter()
    metrics = fit.evaluate(net, samples, data, validation, 8, 'wdl', scale, 0)
    validation_s = time.perf_counter()-start
    sha = hashlib.sha256()
    for param in net.parameters():
        sha.update(memoryview(param.detach().numpy()))
    print(json.dumps(dict(train_s=train_s, validation_s=validation_s,
                          peak_rss_kib=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                          weights_sha256=sha.hexdigest(), losses=losses, metrics=metrics,
                          torch=torch.__version__, numpy=np.__version__, cpu=a.cpu)))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('dataset', type=Path)
    p.add_argument('--baseline-rev', default='6b3397c0409a48bd1fa2c82deb4888b4a26c39f6')
    p.add_argument('--widths', type=int, nargs='+', default=[512, 1536])
    p.add_argument('--pairs', type=int, default=3)
    p.add_argument('--steps', type=int, default=8)
    p.add_argument('--validation-samples', type=int, default=64)
    p.add_argument('--seed', type=int, default=20260925)
    p.add_argument('--cpu', type=int, default=min(os.sched_getaffinity(0)))
    p.add_argument('--out', type=Path)
    p.add_argument('--module-dir', type=Path, help=argparse.SUPPRESS)
    p.add_argument('--width', type=int, help=argparse.SUPPRESS)
    a = p.parse_args()
    if min(a.pairs, a.steps, a.validation_samples, *a.widths) < 1:
        p.error('pairs, steps, validation samples and widths must be positive')
    if a.module_dir:
        worker(a)
        return
    if a.out is None or a.out.exists():
        p.error('--out must name a new result file')
    baseline = subprocess.check_output(['git', 'rev-parse', a.baseline_rev+'^{commit}'], cwd=REPO, text=True).strip()
    archive = subprocess.check_output(['git', 'archive', baseline, 'training/nnue'], cwd=REPO)
    rows = []
    with tempfile.TemporaryDirectory(prefix='nnue-cpu-benchmark-') as temp:
        with tarfile.open(fileobj=io.BytesIO(archive)) as tf:
            tf.extractall(temp, filter='data')
        for width in a.widths:
            for pair in range(a.pairs):
                variants = ['baseline', 'candidate'] if pair % 2 == 0 else ['candidate', 'baseline']
                results = {}
                for variant in variants:
                    directory = Path(temp)/'training/nnue' if variant == 'baseline' else Path(__file__).parent
                    command = [sys.executable, str(Path(__file__).resolve()), str(a.dataset.resolve()),
                               '--module-dir', str(directory), '--width', str(width), '--steps', str(a.steps),
                               '--seed', str(a.seed+pair), '--cpu', str(a.cpu),
                               '--validation-samples', str(a.validation_samples)]
                    result = subprocess.run(command, check=True, capture_output=True, text=True, timeout=300)
                    results[variant] = json.loads(result.stdout)
                for key in ['weights_sha256', 'losses', 'metrics']:
                    if results['baseline'][key] != results['candidate'][key]:
                        raise AssertionError(f'width {width}, pair {pair}: {key} changed')
                row = dict(width=width, pair=pair, results=results)
                rows.append(row)
                print(json.dumps(dict(width=width, pair=pair,
                      train_ratio=results['candidate']['train_s']/results['baseline']['train_s'],
                      validation_ratio=results['candidate']['validation_s']/results['baseline']['validation_s'])), flush=True)
    summary = {}
    for width in a.widths:
        selected = [r for r in rows if r['width'] == width]
        summary[str(width)] = {metric:statistics.median(
            r['results']['candidate'][metric]/r['results']['baseline'][metric] for r in selected)
            for metric in ['train_s', 'validation_s', 'peak_rss_kib']}
    report = dict(baseline_revision=baseline,
                  candidate_revision=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(),
                  candidate_source_sha256={name:hashlib.sha256((Path(__file__).parent/name).read_bytes()).hexdigest()
                                           for name in ('pilot_fit.py', 'train.py', 'benchmark_training.py')},
                  dataset=json.loads((a.dataset/'dataset.json').read_text()), seed=a.seed,
                  steps=a.steps, validation_samples=a.validation_samples,
                  cpu=a.cpu, raw=rows, median_candidate_over_baseline=summary)
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
