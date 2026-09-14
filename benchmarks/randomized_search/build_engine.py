#!/usr/bin/env python3
"""Build the pinned experimental fork without editing the checkout or normal binary."""
import argparse, hashlib, json, shutil, subprocess, tarfile, tempfile
from pathlib import Path

BASE = '9d3058696779ed1664c54e336d351c1ef006acdf'
HERE = Path(__file__).resolve().parent

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out',type=Path,required=True)
    p.add_argument('--target-dir',type=Path)
    args=p.parse_args();args.out.mkdir(parents=True,exist_ok=False)
    repo=Path(subprocess.check_output(['git','rev-parse','--show-toplevel'],cwd=HERE,text=True).strip())
    with tempfile.TemporaryDirectory(prefix='random-search-build-') as temp:
        work=Path(temp);archive=work/'base.tar'
        with archive.open('wb') as f:subprocess.run(['git','archive',BASE],cwd=repo,stdout=f,check=True)
        with tarfile.open(archive) as f:f.extractall(work,filter='data')
        patch=HERE/'engine.patch'
        subprocess.run(['git','apply','--check',str(patch)],cwd=work,check=True)
        subprocess.run(['git','apply',str(patch)],cwd=work,check=True)
        (work/'examples').mkdir(exist_ok=True)
        shutil.copyfile(HERE/'search_bench.rs',work/'examples/search_bench.rs')
        command=['cargo','build','--locked','--release','--features','search-experiments','--example','search_bench']
        target=args.target_dir.resolve() if args.target_dir else work/'target'
        command+=['--target-dir',str(target)]
        subprocess.run(command,cwd=work,check=True)
        binary=args.out/'search_bench';shutil.copy2(target/'release/examples/search_bench',binary)
        provenance=dict(base_revision=BASE,tooling_revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),
            patch_sha256=hashlib.sha256(patch.read_bytes()).hexdigest(),harness_sha256=hashlib.sha256((HERE/'search_bench.rs').read_bytes()).hexdigest(),
            binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),compiler=subprocess.check_output(['rustc','-Vv'],text=True),command=command)
        (args.out/'build.json').write_text(json.dumps(provenance,indent=2)+'\n')
    print(binary)

if __name__=='__main__':main()
