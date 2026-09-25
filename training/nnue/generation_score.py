"""Score an exported model after its trainer exits, avoiding two float input tables."""
import argparse
import json
from pathlib import Path
import numpy as np
import torch
from pilot_fit import from_quantized, calibration, evaluate
from train import atomic_json


def main():
    p=argparse.ArgumentParser();p.add_argument('dataset',type=Path);p.add_argument('model',type=Path)
    a=p.parse_args();torch.set_num_threads(1);torch.set_num_interop_threads(1)
    samples=[json.loads(x) for x in (a.dataset/'samples.jsonl').read_text().splitlines()]
    data=np.memmap(a.dataset/'features.bin',mode='r',dtype='<u4')
    scale=calibration(samples)['scale'];schema=json.loads((a.dataset/'schema.json').read_text())
    validation=[i for i,s in enumerate(samples) if s['split']=='validation']
    test=[i for i,s in enumerate(samples) if s['split']=='test']
    class Material(torch.nn.Module):
        def forward(self,x,off):return torch.zeros((len(off)-1)//2)
    baseline=evaluate(Material(),samples,data,validation,8,'wdl',scale,0)
    cp=json.loads(a.model.read_text());net=from_quantized(a.model.parent/cp['weights']['nnue']['file'],schema['features'])
    val=evaluate(net,samples,data,validation,1,'wdl',scale,0)
    if val['objective'] >= baseline['objective']:
        raise ValueError('Exported network did not beat fixed material on validation')
    result=evaluate(net,samples,data,test,1,'wdl',scale,0)
    atomic_json(a.model.parent/'test.json',result)
    atomic_json(a.model.parent/'quality.json',dict(validation=val,material_validation=baseline,test=result))
    status=json.loads((a.model.parent/'status.json').read_text());status.update(state='completed',test=result)
    atomic_json(a.model.parent/'status.json',status)
    print(json.dumps(dict(validation=val,material_validation=baseline,test=result)),flush=True)

if __name__=='__main__':main()
