"""Independent NumPy reference for the version-1 Rust inference format."""
import json,struct
import numpy as np
class Quantized:
 def __init__(self,path,features):
  self.data=np.memmap(path,mode='r',dtype='u1');b=self.data
  assert bytes(b[:8])==b'TKNNUE01';version,self.width,channels=struct.unpack('<III',b[8:20]);assert version==1 and channels==92
  assert struct.unpack('<III',b[52:64])==(4096,64,1)
  self.offset=64
  def take(dtype,shape):
   size=np.dtype(dtype).itemsize*int(np.prod(shape));a=np.ndarray(shape,dtype,buffer=b,offset=self.offset);self.offset+=size;return a
  w=self.width;self.weights=take('<i2',(features,w));self.bias=take('<i4',(w,));self.h1=take('i1',(32,2*w));self.b1=take('<i4',(32,));self.h2=take('i1',(32,32));self.b2=take('<i4',(32,));self.out=take('<f4',(32,));self.out_bias=take('<f4',(1,))[0];assert self.offset==len(b)
 def residual(self,us,them):
  sums=[self.weights[ids].sum(axis=0,dtype=np.int32)+self.bias for ids in [us,them]]
  x=np.clip((np.concatenate(sums).astype(np.int64)*127+2048)//4096,0,127).astype(np.int32)
  h=np.clip((self.h1.astype(np.int32)@x+self.b1+32)//64,0,127)
  h=np.clip((self.h2.astype(np.int32)@h+self.b2+32)//64,0,127)
  # Match Rust's f32 accumulation order rather than a reassociated BLAS dot.
  value=np.float32(self.out_bias)
  for a,b in zip(h,self.out):value=np.float32(value+np.float32(np.float32(a)/np.float32(127))*b)
  score=np.float32(value*np.float32(1000));rounded=np.floor(score+.5) if score>=0 else np.ceil(score-.5)
  return int(np.clip(rounded,-100000,100000))
