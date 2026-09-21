import argparse,difflib,hashlib,json,pathlib,shutil,subprocess,tarfile
from variants import apply
ROOT=pathlib.Path(__file__).resolve().parents[2]; HERE=pathlib.Path(__file__).resolve().parent
OUT=ROOT/'data/derived/search-speed-20260920'
p=argparse.ArgumentParser();p.add_argument('variants',nargs='+');args=p.parse_args()
base=json.loads((OUT/'corpus.json').read_text())['revision']
work=OUT/'work';work.mkdir(exist_ok=True);target=OUT/'target';binaries=OUT/'bin';binaries.mkdir(exist_ok=True)
archive=OUT/'base.tar'
with archive.open('wb') as f:subprocess.run(['git','archive',base],cwd=ROOT,stdout=f,check=True)
for variant in args.variants:
 with tarfile.open(archive) as f:f.extractall(work,filter='data')
 apply(work,variant)
 (work/'examples').mkdir(exist_ok=True);shutil.copy(HERE/'harness.rs',work/'examples/speed_experiment.rs')
 log=OUT/f'build-{variant}.log'
 cmd=['cargo','build','--offline','--locked','--release','--example','speed_experiment','--target-dir',str(target),'-j','2']
 with log.open('w') as f:
  run=subprocess.run(cmd,cwd=work,stdout=f,stderr=subprocess.STDOUT)
 if run.returncode:raise RuntimeError(f'{variant} build failed: {log}')
 dest=binaries/variant;shutil.copy2(target/'release/examples/speed_experiment',dest)
 sources={str(p.relative_to(work)):hashlib.sha256(p.read_bytes()).hexdigest() for p in (work/'src').rglob('*.rs')}
 (OUT/f'build-{variant}.json').write_text(json.dumps(dict(variant=variant,base=base,binary_sha256=hashlib.sha256(dest.read_bytes()).hexdigest(),sources=sources,command=cmd,compiler=subprocess.check_output(['rustc','-Vv'],text=True)),indent=2))
 patch=[]
 with tarfile.open(archive) as baseline:
  for p in sorted((work/'src').rglob('*.rs')):
   name=str(p.relative_to(work)); original=baseline.extractfile(name).read().decode()
   changed=p.read_text()
   if original!=changed:patch.extend(difflib.unified_diff(original.splitlines(True),changed.splitlines(True),fromfile='a/'+name,tofile='b/'+name))
 (OUT/f'{variant}.patch').write_text(''.join(patch))
 print('Built',variant,flush=True)
