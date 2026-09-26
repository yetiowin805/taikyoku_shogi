"""Assemble a std-only microbenchmark using pinned, actual movement configs."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from variants import BASE, balanced_end
root=Path(__file__).resolve().parents[2]
out=Path(sys.argv[1]);out.mkdir(parents=True, exist_ok=True)
def source(path): return subprocess.check_output(['git','show',f'{BASE}:{path}'],cwd=root,text=True)
piece=source('src/piece.rs');a=piece.index('pub enum PieceType {');enum=piece[a:balanced_end(piece,piece.index('{',a))]
eval=source('src/eval.rs');a=eval.index('pub const ALL_PIECE_TYPES:');b=eval.index('];',a)+2
parts=['#![allow(dead_code,unused_imports)]\nmod piece {\n#[derive(Debug,Clone,Copy,PartialEq,Eq,Hash)]\n'+enum+'\n#[derive(Clone,Copy)] pub struct Piece { pub piece_type:PieceType, pub is_promoted:bool, pub base_piece_type:Option<PieceType> }\n}\n', 'mod eval { use crate::piece::PieceType;\n'+eval[a:b]+'\n}\n']
hashes={}
for module in ['direction','types','config']:
    original=source(f'src/movement/{module}.rs');p=out/f'{module}.rs';p.write_text(original)
    hashes[module]=hashlib.sha256(original.encode()).hexdigest()
parts.append('mod movement {\n'+''.join(f'#[path={json.dumps(str((out/f"{m}.rs").resolve()))}] pub mod {m};\n' for m in ['direction','types','config'])+'}\n')
parts.append((Path(__file__).with_name('micro.rs')).read_text())
(out/'main.rs').write_text(''.join(parts))
(out/'sources.json').write_text(json.dumps(dict(base=BASE,source_hashes=hashes),indent=2)+'\n')
print(out/'main.rs')
