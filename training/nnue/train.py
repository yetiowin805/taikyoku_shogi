"""CPU starter NNUE trainer. Sparse SGD embeddings + Adam dense head; game-held-out validation.
No downloads, tournament mutations, or checkpoint replacement occur here.
"""
import argparse,hashlib,json,math,os,random,re,signal,struct,time
from pathlib import Path
import numpy as np
import torch
from torch import nn

from quantized import Quantized
from plateau import Plateau, Policy
from dataclasses import asdict

STOPPING=False
def stop_handler(*_):
 global STOPPING
 STOPPING=True
def check_stop():
 if STOPPING:raise InterruptedError("Training interrupted; resume from the last completed epoch")
def atomic_json(path,value):
 tmp=path.with_suffix(".tmp");tmp.write_text(json.dumps(value,indent=2)+"\n");tmp.replace(path)

def restore_net(checkpoint,features,width):
 # mmap + meta + assign avoids allocating a second, gigabyte-sized input table.
 with torch.device("meta"):net=Net(features,width)
 net.load_state_dict(checkpoint["net"],assign=True)
 return net

def prune_blob(out,old_blob):
 if not old_blob or not re.fullmatch(r"nnue-[0-9a-f]{64}\.bin",old_blob.name) or old_blob.parent.resolve()!=out.resolve():return
 for cp in out.glob("NNUE_W*.json"):
  desc=json.loads(cp.read_text())["weights"].get("nnue")
  if desc and (cp.parent/desc["file"]).resolve()==old_blob.resolve():return
 old_blob.unlink(missing_ok=True)
WIDTHS=[512,768,1024,1536,2048]
def digest(path):
 h=hashlib.sha256()
 with path.open('rb') as f:
  for chunk in iter(lambda:f.read(1<<20),b''):h.update(chunk)
 return h.hexdigest()

class Net(nn.Module):
 def __init__(self,features,width):
  super().__init__();self.width=width
  self.embedding=nn.EmbeddingBag(features,width,mode='sum',sparse=True,include_last_offset=True)
  nn.init.uniform_(self.embedding.weight,-.003,.003)
  self.bias=nn.Parameter(torch.full((width,),.5))
  self.h1=nn.Linear(width*2,32);self.h2=nn.Linear(32,32);self.output=nn.Linear(32,1)
  nn.init.constant_(self.h1.bias,.1);nn.init.constant_(self.h2.bias,.1)
  nn.init.normal_(self.output.weight,std=.01);nn.init.zeros_(self.output.bias)
 def forward(self,indices,offsets):
  x=(self.embedding(indices,offsets)+self.bias).clamp(0,1).reshape(-1,self.width*2)
  return self.output(self.h2(self.h1(x-.5).clamp(0,1)).clamp(0,1)).squeeze(-1)

def batch(samples,data,indices):
 arrays=[];offsets=[0]
 for i in indices:
  s=samples[i];start=s['offset'];middle=start+s['us'];end=middle+s['them']
  for begin,stop in [(start,middle),(middle,end)]:
   arrays.append(data[begin:stop]);offsets.append(offsets[-1]+stop-begin)
 x=torch.from_numpy(np.concatenate(arrays).astype(np.int64));off=torch.tensor(offsets,dtype=torch.long)
 target=torch.tensor([(samples[i]['score']-samples[i]['material'])/1000 for i in indices],dtype=torch.float32).clamp(-100,100)
 return x,off,target

def validate(net,samples,data,ids,batch_size):
 errors=[];base=[];loss=[]
 with torch.no_grad():
  for start in range(0,len(ids),batch_size):
   check_stop();group=ids[start:start+batch_size];x,o,y=batch(samples,data,group);pred=net(x,o);delta=(pred-y).double()
   errors.extend(delta.tolist());base.extend(y.double().tolist());loss.extend(torch.nn.functional.smooth_l1_loss(pred,y,reduction='none').tolist())
 return dict(huber=float(np.mean(loss)),mae=float(np.mean(np.abs(errors)))*1000,material_mae=float(np.mean(np.abs(base)))*1000,rmse=float(np.sqrt(np.mean(np.square(errors))))*1000,count=len(ids))

def export(net,path,schema):
 """Bounded-memory export: int16 /4096 feature weights, int8 /64 heads."""
 tmp=path.with_suffix('.tmp')
 with tmp.open('wb') as f:
  f.write(b'TKNNUE01');f.write(struct.pack('<III',1,net.width,schema['channels']));f.write(bytes.fromhex(schema['hash']));f.write(struct.pack('<III',4096,64,1))
  w=net.embedding.weight.detach().numpy()
  for start in range(0,len(w),256):
   f.write(np.rint(w[start:start+256]*4096).clip(-2048,2048).astype('<i2').tobytes())
  f.write(np.rint(net.bias.detach().numpy()*4096).astype('<i4').tobytes())
  for layer in [net.h1,net.h2]:
   qw=np.rint(layer.weight.detach().numpy()*64).clip(-127,127).astype('i1')
   bias=layer.bias.detach().numpy()*127*64
   # Fold training-time centering into the first integer bias; inference architecture is unchanged.
   if layer is net.h1:bias=bias-qw.astype(np.int32).sum(axis=1)*63.5
   f.write(qw.tobytes());f.write(np.rint(bias).astype('<i4').tobytes())
  f.write(net.output.weight.detach().numpy().astype('<f4').tobytes());f.write(net.output.bias.detach().numpy().astype('<f4').tobytes());f.flush();os.fsync(f.fileno())
 tmp.replace(path)
 h=hashlib.sha256()
 with path.open('rb') as f:
  for chunk in iter(lambda:f.read(1<<20),b''):h.update(chunk)
 return h.hexdigest()

def main():
 p=argparse.ArgumentParser();p.add_argument('dataset',type=Path);p.add_argument('--base',type=Path,required=True);p.add_argument('--out',type=Path,required=True)
 p.add_argument('--width',type=int,choices=WIDTHS+[32],required=True);p.add_argument('--epochs',type=int,default=2);p.add_argument('--batch',type=int,default=4);p.add_argument('--threads',type=int,default=4);p.add_argument('--seed',type=int,default=20260920);p.add_argument('--limit',type=int);p.add_argument('--resume',action='store_true');p.add_argument('--embedding-lr',type=float,default=.0002);p.add_argument('--head-lr',type=float,default=.0003)
 p.add_argument('--resume-from',type=Path);p.add_argument('--seed-model',type=Path);p.add_argument('--generation',default='v1');p.add_argument('--plateau',action='store_true');p.add_argument('--min-epochs',type=int,default=8);p.add_argument('--patience',type=int,default=3);p.add_argument('--min-improvement',type=float,default=.005);p.add_argument('--lr-factor',type=float,default=.5);p.add_argument('--lr-reductions',type=int,default=2);p.add_argument('--prune-exports',action='store_true')
 a=p.parse_args()
 if not re.fullmatch(r'[A-Za-z0-9_-]+',a.generation):raise ValueError('Invalid generation')
 if a.resume and a.resume_from:raise ValueError('Choose --resume or --resume-from')
 if a.resume_from and (not a.seed_model or a.out.resolve()==a.resume_from.parent.resolve()):raise ValueError('Forking requires --seed-model and a separate output directory')
 policy=Policy(a.min_epochs,a.epochs,a.patience,a.min_improvement,a.lr_factor,a.lr_reductions) if a.plateau else None
 signal.signal(signal.SIGTERM,stop_handler);signal.signal(signal.SIGINT,stop_handler)
 torch.set_num_interop_threads(1);torch.set_num_threads(a.threads);torch.manual_seed(a.seed);np.random.seed(a.seed);random.seed(a.seed)
 schema=json.loads((a.dataset/'schema.json').read_text());samples=[json.loads(s) for s in (a.dataset/'samples.jsonl').read_text().splitlines()]
 data=np.memmap(a.dataset/'features.bin',mode='r',dtype='<u4');train=[i for i,s in enumerate(samples) if s['split']=='train'];val=[i for i,s in enumerate(samples) if s['split']=='validation'];rng=random.Random(a.seed);rng.shuffle(train);rng.shuffle(val)
 if a.limit:train=train[:a.limit];val=val[:max(16,a.limit//10)]
 assert len(train)>=16 and len(val)>=8,'Insufficient train/validation samples'
 assert not ({samples[i]['game'] for i in train}&{samples[i]['game'] for i in val})
 a.out.mkdir(parents=True,exist_ok=True);resume=a.out/f'w{a.width}.training.pt';report_path=a.out/f'w{a.width}.metrics.json'
 if report_path.exists() and not a.resume:raise ValueError('Output exists; use a fresh directory or --resume')
 identity=json.loads((a.dataset/'dataset.json').read_text())
 for name in ['features','samples','schema']:
  suffix={'features':'bin','samples':'jsonl','schema':'json'}[name]
  assert digest(a.dataset/f'{name}.{suffix}')==identity[name+'_sha256'],'Dataset changed'
 assert digest(a.base)==identity['baseline_sha256'],'Material baseline changed'
 dataset_id=hashlib.sha256(json.dumps(identity,sort_keys=True).encode()).hexdigest();base=json.loads(a.base.read_text());assert not base['weights'].get('nnue')
 recipe=dict(version=1,width=a.width,seed=a.seed,batch=a.batch,embedding_lr=a.embedding_lr,head_lr=a.head_lr,train_count=len(train),validation_count=len(val))
 if (a.out/'DISCARDED.json').exists():raise ValueError('This pilot was discarded; use a fresh output directory')
 start_epoch=0;metrics=[];best=float('inf');start_time=time.monotonic();checkpoint=None;tracker=None
 source=resume if a.resume else a.resume_from
 if source:
  checkpoint=torch.load(source,map_location='cpu',weights_only=True,mmap=True)
  assert checkpoint['dataset_id']==dataset_id and checkpoint['recipe']==recipe,'Resume configuration changed'
  net=restore_net(checkpoint,schema['features'],a.width)
  start_epoch=checkpoint['epoch'];metrics=checkpoint['metrics'];best=checkpoint['best']
  if a.resume and checkpoint.get('generation',a.generation)!=a.generation:raise ValueError('Generation changed on resume')
  if checkpoint.get('plateau'):
   assert policy and checkpoint['plateau']['policy']==asdict(policy),'Plateau policy changed on resume'
   tracker=Plateau(**checkpoint['plateau']['state'])
  if a.resume_from:
   cp=json.loads(a.seed_model.read_text());d=cp['weights']['nnue'];blob=(a.seed_model.parent/d['file']).resolve()
   assert d['width']==a.width and d['feature_hash']==schema['names_hash'] and digest(blob)==d['sha256'],'Seed model changed'
   assert cp['weights']['piece']==base['weights']['piece'],'Seed material changed'
   cp['name']=f'NNUE_W{a.width}_{a.generation}';d['file']=str(blob)
   atomic_json(a.out/f'{cp["name"]}.json',cp)
 else:net=Net(schema['features'],a.width)
 # Optimizers must be constructed AFTER assign=True, to reference the loaded parameters.
 emb=torch.optim.SGD([net.embedding.weight],lr=a.embedding_lr)
 dense=torch.optim.Adam([p for n,p in net.named_parameters() if n!='embedding.weight'],lr=a.head_lr)
 if checkpoint:
  emb.load_state_dict(checkpoint['embedding_optimizer']);dense.load_state_dict(checkpoint['dense_optimizer']);del checkpoint
 else:
  initial=validate(net,samples,data,val,a.batch);metrics.append(dict(epoch=0,validation=initial));print(json.dumps(dict(width=a.width,initial=initial)),flush=True)
 if policy and tracker is None:tracker=Plateau(best=best if math.isfinite(best) else metrics[-1]['validation']['huber'])
 status_path=a.out/f'w{a.width}.status.json'
 def status(state,**extra):atomic_json(status_path,dict(state=state,width=a.width,completed_epoch=start_epoch,updated=time.time(),**extra))
 if tracker and tracker.stopped:
  status('completed',reason=tracker.stopped);return
 if start_epoch>=a.epochs:
  status('completed',reason='max_epochs');return
 status('training')
 for epoch in range(start_epoch,a.epochs):
  order=train.copy();random.Random(a.seed+epoch).shuffle(order);running=0
  for step,start in enumerate(range(0,len(order),a.batch)):
   check_stop();ids=order[start:start+a.batch];x,o,y=batch(samples,data,ids);emb.zero_grad(set_to_none=True);dense.zero_grad(set_to_none=True)
   pred=net(x,o);loss=torch.nn.functional.smooth_l1_loss(pred,y);assert torch.isfinite(loss),'Nonfinite loss';loss.backward()
   # Sparse gradients retain bounded memory; no dense Adam state for the huge input table.
   emb.step();dense.step()
   with torch.no_grad():
    # Clamp only touched embedding rows; scanning the entire table each step is prohibitive.
    touched=torch.unique(x);net.embedding.weight[touched]=net.embedding.weight[touched].clamp(-.5,.5)
    net.bias.clamp_(-2,2)
    for layer in [net.h1,net.h2]:layer.weight.clamp_(-127/64,127/64);layer.bias.clamp_(-8,8)
   running+=loss.item()*len(ids)
   if step%100==0:
    status('training',epoch=epoch+1,samples=start+len(ids),total_samples=len(train));print(json.dumps(dict(width=a.width,epoch=epoch+1,step=step,samples=start+len(ids),loss=running/(start+len(ids)),elapsed_s=time.monotonic()-start_time)),flush=True)
  status('validating',epoch=epoch+1);validation=validate(net,samples,data,val,a.batch);metrics.append(dict(epoch=epoch+1,train_huber=running/len(train),validation=validation))
  if validation['huber']<best:
   best=validation['huber'];blob=a.out/f'w{a.width}-candidate.bin';sha=export(net,blob,schema)
   quant=Quantized(blob,schema['features']);errors=[]
   with torch.no_grad():
    for i in val[:32]:
     sample=samples[i];begin=sample['offset'];middle=begin+sample['us'];end=middle+sample['them'];x,o,_=batch(samples,data,[i])
     errors.append(abs(quant.residual(data[begin:middle],data[middle:end])-net(x,o).item()*1000))
   quantization=dict(mean_abs_error=float(np.mean(errors)),max_abs_error=float(max(errors)),positions=len(errors))
   assert quantization['max_abs_error']<250 and quantization['mean_abs_error']<50,quantization
   metrics[-1]['quantization']=quantization
   del quant
   # Content-addressed files keep the old JSON checkpoint valid until the new
   # descriptor has passed every export check and is atomically published.
   published=a.out/f'nnue-{sha}.bin'
   blob.replace(published);blob=published
   cp=json.loads(a.base.read_text());cp['format_version']=2;cp['name']=f'NNUE_W{a.width}_{a.generation}';cp['created_at']=f'unix:{int(time.time())}'
   cp['weights']['nnue']=dict(file=blob.name,sha256=sha,width=a.width,feature_hash=schema['names_hash']);cp['weights']['noise_scale']=0
   cp['weights']['lr_flight_k']=0;cp['weights']['two_mover_align_k']=0;cp['search_defaults']['q_own_large_only']=True
   target=a.out/f'{cp["name"]}.json';old_blob=None
   if target.exists():old_blob=target.parent/json.loads(target.read_text())['weights']['nnue']['file']
   atomic_json(target,cp)
   if a.prune_exports:prune_blob(a.out,old_blob)
  decision=tracker.observe(epoch+1,validation['huber'],policy) if tracker else ('max_epochs' if epoch+1>=a.epochs else 'continue')
  if decision=='reduce_lr':
   for optimizer in [emb,dense]:
    for group in optimizer.param_groups:group['lr']*=policy.lr_factor
  metrics[-1]['decision']=decision;metrics[-1]['next_learning_rates']=[emb.param_groups[0]['lr'],dense.param_groups[0]['lr']]
  tmp=resume.with_suffix('.tmp');torch.save(dict(generation=a.generation,plateau=tracker.record(policy) if tracker else None,net=net.state_dict(),embedding_optimizer=emb.state_dict(),dense_optimizer=dense.state_dict(),epoch=epoch+1,width=a.width,seed=a.seed,dataset_id=dataset_id,recipe=recipe,metrics=metrics,best=best),tmp);tmp.replace(resume)
  report=dict(width=a.width,dataset_id=dataset_id,seed=a.seed,train_positions=len(train),validation_positions=len(val),torch_version=torch.__version__,epochs=metrics,best_huber=best,elapsed_s=time.monotonic()-start_time,architecture=f'{a.width}x2-32-32-1',baseline='fixed material',target='saved black-absolute search score converted to STM; residual /1000, capped +/-100',optimizer=f'sparse SGD {a.embedding_lr} + dense Adam {a.head_lr}; centered head input',threads=a.threads)
  report['generation']=a.generation;report['stop_reason']=decision if decision in ('plateau','max_epochs') else None
  atomic_json(report_path,report);print(json.dumps(report),flush=True)
  start_epoch=epoch+1;status('completed' if report['stop_reason'] else 'training',reason=report['stop_reason'],validation=validation,learning_rates=metrics[-1]['next_learning_rates'])
  if report['stop_reason']:break
if __name__=='__main__':
 try:main()
 except InterruptedError as e:
  print(str(e),flush=True);raise SystemExit(130)
