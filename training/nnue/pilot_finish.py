"""Supervise already-running local pilot jobs, then check and play all four candidates.

An interrupted invocation resumes completed matches. A stalled/failed trainer is
reported instead of silently waiting indefinitely. No live VPS controls are used.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import time

from pilot_compare import matches, verify


def main():
    p=argparse.ArgumentParser()
    p.add_argument('root',type=Path)
    p.add_argument('--wait-hours',type=float,default=5)
    p.add_argument('--cpus',default='4,6,8,10')
    p.add_argument('--available-models',action='store_true',
                   help='Evaluate saved best checkpoints, including budget-limited training')
    a=p.parse_args()
    cpus=[int(x) for x in a.cpus.split(',')]
    names=['warm-huber','fresh-huber','warm-wdl','warm-wdl10']
    if a.available_models:
        names=[name for name in names if (a.root/name/'model.json').exists()]
        if not names:raise RuntimeError('No valid exported models to compare')
    deadline=time.monotonic()+a.wait_hours*3600
    while not a.available_models:
        complete=True
        for name in names:
            path=a.root/name/'status.json'
            if not path.exists():
                complete=False;continue
            status=json.loads(path.read_text())
            if status['state']!='completed':
                complete=False
                if time.time()-path.stat().st_mtime>900:
                    raise RuntimeError(f'{name} stopped updating; inspect {a.root}/{name}.log')
        if complete:break
        if time.monotonic()>deadline:
            raise TimeoutError('Training wait expired; jobs were not killed')
        time.sleep(20)
    parent=Path('data/nnue-admission-v2/NNUE_W512_v2.json')
    # One common held-out metric for all loss functions, including the frozen parent.
    import numpy as np
    import torch
    from pilot_fit import from_quantized, calibration, evaluate
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    os.sched_setaffinity(0,{cpus[0]})
    samples=[json.loads(x) for x in (a.root/'dataset/samples.jsonl').read_text().splitlines()]
    schema=json.loads((a.root/'dataset/schema.json').read_text())
    data=np.memmap(a.root/'dataset/features.bin',mode='r',dtype='<u4')
    test=[i for i,s in enumerate(samples) if s['split']=='test']
    scale=calibration(samples)['scale']
    common={}
    for name in ['parent']+names:
        model=parent if name=='parent' else a.root/name/'model.json'
        cp=json.loads(model.read_text())
        net=from_quantized(model.parent/cp['weights']['nnue']['file'],schema['features'])
        common[name]=evaluate(net,samples,data,test,8,'huber',scale,0)
        del net
    (a.root/'common-test.json').write_text(json.dumps(common,indent=2)+'\n')
    baseline_dir=a.root/'parent';baseline_dir.mkdir(exist_ok=True)
    descriptor=json.loads(parent.read_text())
    descriptor['weights']['nnue']['file']=str((parent.parent/descriptor['weights']['nnue']['file']).resolve())
    baseline_model=baseline_dir/'model.json'
    baseline_model.write_text(json.dumps(descriptor,indent=2)+'\n')
    verify(a.root,baseline_model,cpus[0])
    # Homogeneous efficiency cores; one game thread per physical CPU, after training.
    def check_and_play(item):
        name,cpu=item
        model=a.root/name/'model.json'
        try:
            verify(a.root,model,cpu)
            matches(a.root,model,parent,cpu)
            result=dict(test=common[name],matches=json.loads((a.root/name/'matches-summary.json').read_text()))
        except Exception as exc:
            result=dict(test=common[name],error=str(exc))
        (a.root/name/'comparison.json').write_text(json.dumps(result,indent=2)+'\n')
        return name,result
    with ThreadPoolExecutor(max_workers=len(cpus)) as pool:
        report=dict(pool.map(check_and_play,[(name,cpus[i%len(cpus)]) for i,name in enumerate(names)]))
    (a.root/'comparison.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report),flush=True)
    if any('error' in result for result in report.values()):
        raise RuntimeError('One or more comparisons failed; see comparison.json')


if __name__=='__main__':main()
