"""Compare the exported v4 and its frozen parent on identical held-out data."""
import argparse,gc,json,subprocess
from pathlib import Path
import numpy as np
import torch
from pilot_data import digest
from pilot_fit import from_quantized,evaluate
from quantized import Quantized
from v4_data import Features,SCALE,bucket


def main():
    p=argparse.ArgumentParser()
    for k in ('dataset','parent','candidate','binary','out'):p.add_argument('--'+k,type=Path,required=True)
    a=p.parse_args();torch.set_num_threads(1);torch.set_num_interop_threads(1)
    samples=[json.loads(x) for x in (a.dataset/'samples.jsonl').read_text().splitlines()]
    schema=json.loads((a.dataset/'schema.json').read_text());data=Features(a.dataset)
    report={}
    for name,path in [('parent',a.parent),('candidate',a.candidate)]:
        cp=json.loads(path.read_text());d=cp['weights']['nnue'];blob=path.parent/d['file']
        if digest(blob)!=d['sha256']:raise ValueError('Model hash mismatch')
        net=from_quantized(blob,schema['features']);rows={}
        for split in ('validation','test'):
            ids=[i for i,s in enumerate(samples) if s['split']==split]
            rows[split]=evaluate(net,samples,data,ids,8,'wdl',SCALE,0)
            for kind in ('historical','recent','targeted','mate'):
                group=[i for i in ids if bucket(samples[i])==kind]
                if group:rows[split+'/'+kind]=evaluate(net,samples,data,group,8,'wdl',SCALE,0)
        report[name]=rows;del net;gc.collect()
    cp=json.loads(a.candidate.read_text());q=Quantized(a.candidate.parent/cp['weights']['nnue']['file'],schema['features'])
    valid=[s for s in samples if s['split']=='validation'];checks=[]
    for i in np.linspace(0,len(valid)-1,min(8,len(valid)),dtype=int):
        s=valid[i]
        raw=subprocess.run([str(a.binary),'evaluate',str(a.candidate),s['game'],str(s['ply']),'1000'],check=True,capture_output=True,text=True,timeout=60)
        r=json.loads(raw.stdout);begin=s['offset'];mid=begin+s['us']
        expected=round(s['material'])+q.residual(data[begin:mid],data[mid:mid+s['them']])
        if abs(r['score']-expected)>1:raise ValueError('Rust/exported feature parity failed')
        if not r['search']['legal'] or r['search']['depth']<1:raise ValueError('Search sanity failed')
        checks.append(dict(game=s['game'],ply=s['ply'],score=r['score'],search=r['search']))
    report['checks']=checks
    # Checkpoint selection already permits retaining the parent. Do not admit an
    # unchanged copy, or silently publish a materially worse exported candidate.
    parent=json.loads(a.parent.read_text())
    changed=cp['weights']['nnue']['sha256']!=parent['weights']['nnue']['sha256']
    acceptable=report['candidate']['validation']['objective'] <= report['parent']['validation']['objective']*1.01
    report['ready']=bool(changed and acceptable)
    report['candidate_sha256']=digest(a.candidate)
    a.out.write_text(json.dumps(report,indent=2)+'\n')
    if not report['ready']:raise ValueError('Candidate failed admission checks; parent remains active')

if __name__=='__main__':main()
