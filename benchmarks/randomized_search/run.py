#!/usr/bin/env python3
"""Randomized paired three-second searches, balanced over eight BASE checkpoints."""
import argparse, concurrent.futures, gzip, hashlib, json, os, random, signal, subprocess, threading, time
from pathlib import Path

BASELINE='S0-W0-T0'
CONFIGS=[f'S{s}-W{w}-T{t}' for s in [0,1] for w in [0,100,500,2000] for t in [0,1,2]]
STOP=threading.Event()

def plan(cases,rounds=2000,seed=20260914,cpus=(0,1,2,3)):
    rng=random.Random(seed);agents=sorted({c['agent'] for c in cases})
    pools={a:[i for i,c in enumerate(cases) if c['agent']==a] for a in agents};jobs=[]
    for round_id in range(rounds):
        shuffled=agents[:];rng.shuffle(shuffled)
        for j,agent in enumerate(shuffled):
            # Alternate design strata exactly; uniform random factor combinations.
            control=(round_id+j)%2==0
            labels=[BASELINE,rng.choice(CONFIGS[1:])] if control else rng.sample(CONFIGS,2)
            rng.shuffle(labels)
            jobs.append(dict(pair=len(jobs),round=round_id,agent=agent,case=rng.choice(pools[agent]),
                cpu=cpus[(j+round_id)%len(cpus)],variants=labels,control=control))
    return jobs

def run(args):
    STOP.clear()
    cpus=list(map(int,args.cpus.split(',')))
    if len(set(cpus))!=len(cpus) or not set(cpus)<=os.sched_getaffinity(0):raise ValueError('invalid CPU affinity')
    if args.out.exists():raise ValueError('output directory already exists; use a fresh one')
    cases=json.loads(args.corpus.read_text());jobs=plan(cases,seed=args.seed,cpus=cpus)
    binary=args.binary.resolve();corpus=args.corpus.resolve();args.out.mkdir(parents=True)
    started=time.monotonic();deadline=started+args.seconds
    env={k:v for k,v in os.environ.items() if not k.startswith('TAIKYOKU_AB_')}
    provenance=dict(started_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),seconds=args.seconds,
        budget_ms=args.budget_ms,depth=64,seed=args.seed,cpus=cpus,configurations=CONFIGS,
        binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),corpus_sha256=hashlib.sha256(corpus.read_bytes()).hexdigest(),
        cpu=subprocess.check_output(['lscpu'],text=True),tool='randomized_search_v1')
    (args.out/'provenance.json').write_text(json.dumps(provenance,indent=2)+'\n')
    def lane(cpu):
        completed=failures=0;counts={};last_status=0.
        with gzip.open(args.out/f'raw-cpu{cpu}.jsonl.gz','wt') as raw,(args.out/f'stderr-cpu{cpu}.log').open('ab') as err:
            for job in (j for j in jobs if j['cpu']==cpu):
                if STOP.is_set() or time.monotonic()+2*args.process_timeout>=deadline:break
                for order,label in enumerate(job['variants']):
                    t=time.monotonic()
                    try:
                        proc=subprocess.run(['taskset','-c',str(cpu),str(binary),str(corpus),str(job['case']),
                            '64',str(args.budget_ms),label,'1'],stdout=subprocess.PIPE,stderr=err,
                            timeout=args.process_timeout,check=True,env=env)
                        row=json.loads(proc.stdout.splitlines()[-1])
                        if row['meta']['index']!=job['case'] or row['variant']!=label:raise ValueError('uncorrelated result')
                        if row['completed_depth']==0 and row['best'] is not None:raise ValueError('no completed iteration')
                    except (subprocess.SubprocessError,ValueError,IndexError) as exc:
                        row=dict(error=str(exc));failures+=1
                    row.update(pair=job['pair'],round=job['round'],agent=job['agent'],case=job['case'],cpu=cpu,
                        variants=job['variants'],variant=label,order=order,control=job['control'],process_ms=1000*(time.monotonic()-t))
                    raw.write(json.dumps(row,separators=(',',':'))+'\n');raw.flush()
                completed+=1;counts[job['agent']]=counts.get(job['agent'],0)+1
                if failures>=5:STOP.set();raise RuntimeError(f'CPU {cpu}: repeated search failures; see raw rows')
                if time.monotonic()-last_status>=20:
                    status=dict(cpu=cpu,completed_pairs=completed,failures=failures,agents=counts,elapsed_seconds=time.monotonic()-started)
                    tmp=args.out/f'cpu{cpu}.status.tmp';tmp.write_text(json.dumps(status));tmp.replace(args.out/f'cpu{cpu}.status.json')
                    print(json.dumps(status),flush=True);last_status=time.monotonic()
        return dict(cpu=cpu,completed_pairs=completed,failures=failures,agents=counts)
    result=dict(state='failed')
    try:
        with concurrent.futures.ThreadPoolExecutor(max_workers=len(cpus)) as pool:
            futures=[pool.submit(lane,cpu) for cpu in cpus]
            result=dict(state='interrupted' if STOP.is_set() else 'completed',lanes=[f.result() for f in futures])
        result['state']='interrupted' if STOP.is_set() else 'completed'
        if result['state']!='completed':raise RuntimeError('benchmark interrupted')
    finally:
        result['elapsed_seconds']=time.monotonic()-started
        (args.out/'finished.json').write_text(json.dumps(result,indent=2)+'\n')
    return result

def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ['corpus','binary','out']:p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--seconds',type=int,default=10800);p.add_argument('--budget-ms',type=int,default=3000)
    p.add_argument('--process-timeout',type=int,default=15);p.add_argument('--seed',type=int,default=20260914)
    p.add_argument('--cpus',default='0,1,2,3');a=p.parse_args()
    signal.signal(signal.SIGTERM,lambda *_:STOP.set());signal.signal(signal.SIGINT,lambda *_:STOP.set())
    print(json.dumps(run(a),indent=2))

if __name__=='__main__':main()
