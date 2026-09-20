import importlib.util,json,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('field',Path(__file__).with_name('prepare_nnue_field.py'));f=importlib.util.module_from_spec(spec);spec.loader.exec_module(f)
class FieldTests(unittest.TestCase):
 def test_exact_replacement_without_mutating_source(self):
  with tempfile.TemporaryDirectory() as d:
   root=Path(d);old=root/'old.json';old.write_text('{}');source=root/'state.json'
   entries=[dict(id=f'A{i}_Lold',model=str(old)) for i in range(5)]+[dict(id=f'retained-{i}',model=str(old)) for i in range(27)]
   source.write_text(json.dumps(dict(entrants=entries)));before=source.read_bytes();ready=[]
   for w in f.WIDTHS:
    blob=root/f'{w}.bin';blob.write_bytes(str(w).encode());cp=root/f'NNUE_W{w}_v1.json'
    cp.write_text(json.dumps(dict(name=f'NNUE_W{w}_v1',search_defaults={},weights=dict(piece={},nnue=dict(width=w,file=blob.name,sha256=f.digest(blob))))))
    ready.append(dict(width=w,checkpoint_sha256=f.digest(cp),blob_sha256=f.digest(blob),quality=dict(huber=.5,material_huber=1),checks=[{}]*4))
   (root/'ready.json').write_text(json.dumps(dict(widths=ready)));out=root/'manifest.json'
   with patch.object(f.subprocess,'run') as validator:
    result=f.prepare(source,root,out,Path('validator'));self.assertEqual(validator.call_count,32)
   new=json.loads(out.read_text())['entrants'];self.assertEqual(new[:27],entries[5:]);self.assertEqual(len(new),32);self.assertEqual(source.read_bytes(),before);self.assertEqual(len(result['removed']),5)
   with self.assertRaises(ValueError):f.prepare(source,root,out,Path('validator'))
if __name__=='__main__':unittest.main()
