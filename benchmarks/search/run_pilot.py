#!/usr/bin/env python3
"""Small reproducible screen; every search runs sequentially on one pinned CPU."""
import argparse, gzip, hashlib, json, os, random, statistics, subprocess, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).parent / 'pilot'
CORPUS = OUT / 'corpus.json'
BINS = Path('/tmp/taikyoku-bench-bins')
VARIANTS = ['combined', 'swap', 'mobility', 'tt-first', 'tt-promote', 'pvs',
            'asp100', 'asp500', 'asp2000', 'tt-hints', 'tt-bounds', 'clock8', 'clock32', 'clock128']
SIGNATURE = ['completed_depth', 'nodes', 'qnodes', 'score', 'static_eval', 'best', 'root_lines']


def run(label, case, depth, budget, repeat, phase, warm=False):
    binary = BINS / (label if label in ('stock', 'production') else 'experiments')
    args = [str(binary), str(CORPUS), str(case), str(depth), str(budget), label, '1']
    if warm:
        args.append('previous')
    started = time.monotonic()
    with (OUT / 'search.stderr.log').open('ab') as err:
        try:
            result = subprocess.run(['taskset', '-c', str(CPU), *args], stdout=subprocess.PIPE,
                                    stderr=err, cwd=ROOT, timeout=20, check=True)
            row = json.loads(result.stdout.splitlines()[-1])
        except (subprocess.TimeoutExpired, subprocess.CalledProcessError, ValueError) as e:
            row = dict(error=str(e), variant=label, meta=dict(index=case))
    row.update(phase=phase, repeat=repeat, cpu=CPU, warm_previous=warm,
               process_ms=(time.monotonic()-started)*1000)
    with gzip.open(OUT / 'raw.jsonl.gz', 'at') as f:
        f.write(json.dumps(row,separators=(',',':'))+'\n')
    print(json.dumps({k: row.get(k) for k in ['phase','variant','repeat','ms','completed_depth','nodes','score','error']} |
                     dict(case=case)), flush=True)
    return row


def complete(row, depth):
    return not row.get('error') and not row.get('aborted') and row['completed_depth'] == depth


def signature(row):
    return {k: row.get(k) for k in SIGNATURE}


def baseline():
    references = {}
    for repeat in range(2):
        cases=list(range(6)); RNG.shuffle(cases)
        for case in cases:
            labels=['stock','production','combined']; RNG.shuffle(labels)
            rows={label: run(label,case,2,0,repeat,'baseline') for label in labels}
            if not all(complete(r,2) for r in rows.values()):
                raise SystemExit(f'Incomplete mechanical baseline at case {case}')
            if any(signature(r) != signature(rows['stock']) for r in rows.values()):
                (OUT/'baseline-mismatch.json').write_text(json.dumps(rows,indent=2))
                raise SystemExit(f'Mechanical mismatch at case {case}; stop before experimental comparisons')
            references[case]=signature(rows['combined'])
    (OUT/'references.json').write_text(json.dumps(references,indent=2)+'\n')


def screen():
    references=json.loads((OUT/'references.json').read_text())
    for repeat in range(2):
        cases=list(range(6)); RNG.shuffle(cases)
        for case in cases:
            variants=list(VARIANTS); RNG.shuffle(variants)
            for variant in variants:
                row=run(variant,case,2,0,repeat,'depth2')
                if variant in ['combined','mobility','tt-first','clock8','clock32','clock128'] and complete(row,2):
                    if signature(row) != references[str(case)]:
                        raise SystemExit(f'Unexpected fixed-depth mismatch: {variant}, case {case}')
    # Qualification is based on baseline alone; no incomplete depth runs count as speedups.
    qualified=[]
    for case in range(6):
        row=run('combined',case,3,0,0,'qualify_depth3')
        if complete(row,3) and row['ms']<=2000:
            qualified.append(case)
    (OUT/'depth3-selection.json').write_text(json.dumps(qualified)+'\n')
    # One paired deeper run is enough for an initial screen, not a final estimate.
    for case in qualified:
        variants=list(VARIANTS); RNG.shuffle(variants)
        for variant in variants:
            run(variant,case,3,0,0,'depth3')
    for repeat in range(2):
        cases=list(range(6)); RNG.shuffle(cases)
        for case in cases:
            variants=list(VARIANTS); RNG.shuffle(variants)
            for variant in variants:
                run(variant,case,64,1000,repeat,'timed1s')
    # Cross-turn TT tests need a preceding search in the SAME process; compare
    # against equally warmed mechanical searches. Same agent is two plies earlier.
    for repeat in range(2):
        for case in [1,2,3,4,5]:
            variants=['combined','tt-hints','tt-bounds']; RNG.shuffle(variants)
            for variant in variants:
                run(variant,case,3,1000,repeat,'cross_turn',warm=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('phase',choices=['baseline','screen'])
    parser.add_argument('--cpu',type=int,default=0)
    args=parser.parse_args();CPU=args.cpu
    if CPU not in os.sched_getaffinity(0):raise SystemExit('CPU unavailable')
    RNG=random.Random(20260913)
    if args.phase=='baseline':baseline()
    else:screen()
