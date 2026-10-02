#!/usr/bin/env python3
"""Run an instrumented benchmark binary once per NNUE case, without timing claims."""
import json
from pathlib import Path
import subprocess
import sys

binary, corpus, cases, output = map(Path, sys.argv[1:])
with output.open('x') as out, output.with_suffix('.stderr').open('w') as err:
    p = subprocess.Popen(['taskset', '-c', '2', str(binary), str(corpus)],
                         stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=err, text=True)
    try:
        for case in json.loads(cases.read_text()):
            if case['model'] < 2:
                continue
            p.stdin.write(json.dumps(case) + '\n')
            p.stdin.flush()
            row = json.loads(p.stdout.readline())
            out.write(json.dumps(row) + '\n')
            out.flush()
            print(row['agent'], row['name'], row['census']['evals'], flush=True)
    finally:
        p.stdin.close()
        p.wait(timeout=30)
    assert p.returncode == 0
