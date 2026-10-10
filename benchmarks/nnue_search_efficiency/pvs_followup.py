from run import req,run
import random,sys
qs=[]
for p in [0,1,2,6,7,10,16,17,100,102,104,106]:
 vs=['base','guard','pvs1','pvs2','stageb']
 random.Random(8100+p).shuffle(vs)
 qs += [req(p,v,depth=4,budget=30000) for v in vs]
if sys.argv[1]=='deep':run('pvs_deep',qs)
qs=[]
for repeat in range(2):
 for p in [0,1,2,6,7,10,16,17,100,102,104,106,3,11,107,109]:
  vs=['base','pvs1','pvs2']
  random.Random(9100+p+repeat*101).shuffle(vs)
  qs += [req(p,v,depth=8,budget=5000,repeat=repeat) for v in vs]
if sys.argv[1]=='timed':run('timed',qs)
