import importlib.util,json,struct,tempfile,unittest
from pathlib import Path
import numpy as np
import torch
from train import Net,export
from quantized import Quantized

class TrainingTests(unittest.TestCase):
 def test_export_matches_float_and_rejects_truncation(self):
  torch.set_num_threads(1);torch.manual_seed(42)
  net=Net(256,32)
  # Deliberately nonconstant head so agreement is not just a bias-only test.
  with torch.no_grad():net.output.weight.fill_(.2)
  schema=dict(channels=92,hash='00'*32,features=256)
  with tempfile.TemporaryDirectory() as d:
   path=Path(d)/'model.bin';sha=export(net,path,schema);self.assertEqual(len(sha),64)
   quant=Quantized(path,256)
   for shift in range(4):
    us=np.arange(shift,shift+20,dtype=np.uint32);them=np.arange(50+shift,80+shift,dtype=np.uint32)
    x=torch.tensor(np.concatenate([us,them]).astype(np.int64));off=torch.tensor([0,len(us),len(us)+len(them)])
    with torch.no_grad():floating=net(x,off).item()*1000
    self.assertLess(abs(quant.residual(us,them)-floating),20)
   del quant
   path.write_bytes(path.read_bytes()[:-1])
   with self.assertRaises((AssertionError,TypeError,ValueError)):Quantized(path,256)
 def test_sparse_input_gradient_and_learning(self):
  torch.manual_seed(10);net=Net(256,32)
  optimizer=torch.optim.SGD(net.parameters(),lr=.01)
  x=torch.tensor([1,2,3,4]);off=torch.tensor([0,2,4]);target=torch.tensor([.3])
  before=float((net(x,off)-target).square().item())
  for _ in range(20):
   optimizer.zero_grad(set_to_none=True);loss=(net(x,off)-target).square().mean();loss.backward()
   self.assertTrue(net.embedding.weight.grad.is_sparse);optimizer.step()
  self.assertLess(float((net(x,off)-target).square().item()),before)
if __name__=='__main__':unittest.main()
