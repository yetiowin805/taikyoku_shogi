#!/usr/bin/env python3
"""Bounded four-lane, paired 3-second screen. No tournament control in this runner."""
import argparse, concurrent.futures, hashlib, json, os, random, subprocess, time
from pathlib import Path

VARIANTS = ['stock', 'combined', 'swap', 'tt-first', 'tt-promote', 'asp100',
            'asp500', 'asp2000', 'tt-hints', 'clock8', 'clock32', 'clock128']
SIGNATURE = ['completed_depth', 'nodes', 'qnodes', 'score', 'static_eval', 'best', 'root_lines']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--corpus', type=Path, required=True)
    parser.add_argument('--bins', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--cpus', default='0,1,2,3')
    parser.add_argument('--seconds', type=int, default=1200)
    args = parser.parse_args()
    cpus = list(map(int, args.cpus.split(',')))
    assert len(set(cpus)) == len(cpus) and set(cpus) <= os.sched_getaffinity(0)
    args.out.mkdir(parents=True, exist_ok=True)
    cases = json.loads(args.corpus.read_text())
    started = time.monotonic()
    deadline = started + args.seconds
    provenance = dict(started_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
        cpus=cpus, variants=VARIANTS, repeats=3, budget_ms=3000, seed=20260913,
        corpus_sha256=hashlib.sha256(args.corpus.read_bytes()).hexdigest(),
        binaries={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in args.bins.iterdir()},
        cpu=subprocess.check_output(['lscpu'], text=True),
        uname=subprocess.check_output(['uname','-a'], text=True),
        policy='Four concurrent pinned lanes; each case/repeat comparison stays on one CPU. Storage disabled. Warm tests replay two plies earlier with the same checkpoint.')
    (args.out/'provenance.json').write_text(json.dumps(provenance, indent=2)+'\n')

    def run(cpu, case, variant, repeat, phase, depth=64, budget=3000, warm=False):
        if time.monotonic() + 20 > deadline:
            raise TimeoutError('screen wall-clock budget exhausted')
        binary = args.bins / ('stock' if variant == 'stock' else 'experiments')
        command = ['taskset','-c',str(cpu),str(binary),str(args.corpus),str(case),str(depth),str(budget),variant,'1']
        if warm: command.append('previous')
        t = time.monotonic()
        with (args.out/f'stderr-cpu{cpu}.log').open('ab') as err:
            try:
                proc = subprocess.run(command, stdout=subprocess.PIPE, stderr=err, timeout=15, check=True)
                row = json.loads(proc.stdout.splitlines()[-1])
            except (subprocess.SubprocessError, ValueError, IndexError) as exc:
                row = dict(error=str(exc), variant=variant, meta=dict(index=case))
        row.update(cpu=cpu, repeat=repeat, phase=phase, warm_previous=warm, process_ms=(time.monotonic()-t)*1000)
        with (args.out/f'raw-cpu{cpu}.jsonl').open('a') as f:
            f.write(json.dumps(row,separators=(',',':'))+'\n')
        print(json.dumps(dict(cpu=cpu, case=case, variant=variant, repeat=repeat, phase=phase, depth=row.get('completed_depth'), error=row.get('error'))), flush=True)
        return row

    def lane(lane_index, phase):
        cpu = cpus[lane_index]
        rng = random.Random(20260913 + lane_index + (100 if phase=='timed3s' else 0))
        for repeat in range(3 if phase != 'fixed2' else 1):
            indices = [i for i in range(len(cases)) if (i+repeat)%len(cpus)==lane_index]
            rng.shuffle(indices)
            for case in indices:
                if phase=='fixed2':
                    labels=['stock','combined'];rng.shuffle(labels)
                    rows={v:run(cpu,case,v,repeat,phase,depth=2,budget=0) for v in labels}
                    if all(not r.get('error') and r.get('completed_depth')==2 and not r.get('aborted') for r in rows.values()):
                        if any(rows['stock'].get(k)!=rows['combined'].get(k) for k in SIGNATURE):
                            raise RuntimeError(f'Mechanical fixed-depth mismatch at case {case}')
                elif phase=='warm3s':
                    if cases[case]['ply']<2:continue
                    labels=['combined','tt-hints'];rng.shuffle(labels)
                    for v in labels:run(cpu,case,v,repeat,phase,warm=True)
                else:
                    labels=list(VARIANTS);rng.shuffle(labels)
                    for v in labels:run(cpu,case,v,repeat,phase)

    try:
        for phase in ['fixed2','timed3s','warm3s']:
            with concurrent.futures.ThreadPoolExecutor(max_workers=len(cpus)) as pool:
                futures=[pool.submit(lane,i,phase) for i in range(len(cpus))]
                for future in futures:future.result()
    finally:
        (args.out/'finished.json').write_text(json.dumps(dict(elapsed_seconds=time.monotonic()-started),indent=2)+'\n')


if __name__=='__main__': main()
