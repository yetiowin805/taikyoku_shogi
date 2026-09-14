#!/usr/bin/env python3
"""Descriptive smoke results; two timed repeats are not confidence intervals."""
import collections,json,math,statistics,sys
from pathlib import Path
p=Path(sys.argv[1])
def read(name):return [json.loads(l) for l in (p/name).read_text().splitlines()]
micro=read('micro.jsonl');search=read('search.jsonl');desc=read('describe.jsonl')
real={c['name'] for c in json.loads((p/'corpus.json').read_text())}
by={(r['case'],r['rep'],r['variant']):r for r in search}
variants=sorted({r['variant'] for r in search}-{'off'})
result={'micro':{},'search':{},'descriptions':desc}
for case in sorted({r['case'] for r in micro}):
 rows={v:statistics.median(r['ns_per_eval'] for r in micro if r['case']==case and r['variant']==v) for v in ['off']+variants}
 result['micro'][case]={'ns_per_eval':rows,'overhead_pct':{v:100*(n/rows['off']-1) for v,n in rows.items()}}
for variant in variants:
 positions={};changed=deeper=shallower=0
 for case in sorted(real):
  pairs=[(by[(case,rep,'off')],by[(case,rep,variant)]) for rep in range(2)]
  nps=[]
  for b,v in pairs:
   # Early terminal/depth-cap completions are reported but not treated as timed work.
   if min(b['ms'],v['ms'])>=2900 and min(b['nodes'],v['nodes'])>0:nps.append(math.log((v['nodes']/v['ms'])/(b['nodes']/b['ms'])))
   changed+=b['best']!=v['best'];deeper+=v['depth']>b['depth'];shallower+=v['depth']<b['depth']
  positions[case]={'nps_pct':100*math.expm1(statistics.mean(nps)) if nps else None,'depths':[(b['depth'],v['depth']) for b,v in pairs],'scores':[(b['score'],v['score']) for b,v in pairs],'changed_moves':sum(b['best']!=v['best'] for b,v in pairs)}
 effects=[math.log1p(r['nps_pct']/100) for r in positions.values() if r['nps_pct'] is not None]
 result['search'][variant]={'nps_pct':100*math.expm1(statistics.mean(effects)) if effects else None,'positions':positions,'changed_moves':changed,'deeper':deeper,'shallower':shallower}
result['rows']={'micro':len(micro),'search':len(search),'descriptions':len(desc)}
result['max_search_ms']=max(r['ms'] for r in search)
(p/'summary.json').write_text(json.dumps(result,indent=2)+'\n')
for variant,r in result['search'].items():print(variant,'NPS',round(r['nps_pct'],2) if r['nps_pct'] is not None else None,'deeper/shallower/changed',r['deeper'],r['shallower'],r['changed_moves'])
for case,r in result['micro'].items():print(case,'micro ns',{k:round(v,1) for k,v in r['ns_per_eval'].items()})
