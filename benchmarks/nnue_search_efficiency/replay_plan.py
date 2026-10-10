#!/usr/bin/env python3
"""Replay a frozen request list under a new output name; record the new binary hash."""
import argparse,json,pathlib
from run import ROOT,run
ap=argparse.ArgumentParser(description=__doc__)
ap.add_argument('plan',type=pathlib.Path)
ap.add_argument('--name',required=True)
ap.add_argument('--binary',type=pathlib.Path,default=ROOT/'bin/study')
args=ap.parse_args()
if pathlib.Path(args.name).name!=args.name:raise SystemExit('name must be a single file name')
if (ROOT/'results'/f'{args.name}.jsonl').exists():raise SystemExit('Choose a new name; existing measurements are immutable')
plan=json.loads(args.plan.read_text())
run(args.name,plan['requests'],args.binary)
