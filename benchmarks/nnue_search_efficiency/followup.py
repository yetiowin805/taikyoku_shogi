"""Prespecified deeper comparisons after the depth-3 screen; serial searches."""
import random
from run import req,run
qs=[]
# Full-depth comparisons: complete all pairs, balance order across three passes.
for repeat in range(3):
 ps=[0,1,2,6,7,10,16,17,100,102,104,106]
 random.Random(6500+repeat).shuffle(ps)
 for p in ps:
  vs=['base','lazy','qfast','stageb']
  random.Random(5600+repeat*113+p).shuffle(vs)
  qs += [req(p,v,depth=4,budget=60000,repeat=repeat) for v in vs]
run('confirm',qs)
qs=[]
# These are current-search holdouts, not previously untouched game data.
for repeat in range(2):
 for p in [3,4,5,8,9,11,18,19,101,103,105,107,108,109]:
  vs=['base','qfast']
  if (p+repeat)%2:vs.reverse()
  qs += [req(p,v,depth=4,budget=60000,repeat=repeat) for v in vs]
run('holdout',qs)
