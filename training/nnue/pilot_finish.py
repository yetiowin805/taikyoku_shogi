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
    a=p.parse_args()
    names=['warm-huber','fresh-huber','warm-wdl','warm-wdl10']
    deadline=time.monotonic()+a.wait_hours*3600
    while True:
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
    os.sched_setaffinity(0,{4})
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
    # Homogeneous efficiency cores; one game thread per physical CPU, after training.
    def check_and_play(item):
        name,cpu=item
        model=a.root/name/'model.json'
        verify(a.root,model,cpu)
        matches(a.root,model,parent,cpu)
    with ThreadPoolExecutor(max_workers=4) as pool:
        list(pool.map(check_and_play,zip(names,[4,6,8,10])))
    report={name:dict(test=json.loads((a.root/name/'test.json').read_text()),
                      matches=json.loads((a.root/name/'matches-summary.json').read_text()))
            for name in names}
    (a.root/'comparison.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report),flush=True)


if __name__=='__main__':main()
