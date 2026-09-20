import json,pathlib,statistics,math,sys
out=pathlib.Path(sys.argv[1])
def read(phase):return [json.loads(s) for s in (out/f'{phase}.jsonl').read_text().splitlines()]
rows=read('timed');verify=read('verify');micro=read('micro')
variants=['baseline','native','fused','snapshot','both','native-both']
widths=sorted({r['width'] for r in rows});positions=['opening','middlegame','tactical','long-history']
def geo(xs):return math.exp(statistics.mean(math.log(x) for x in xs))
def nps(r):return r['result']['nodes']/(r['result']['elapsed_ns']/1e9)
summary=[]
for w in widths:
 for variant in variants:
  samples=[r for r in rows if r['width']==w and r['variant']==variant]
  if not samples:continue
  paired=[];perpos=[]
  for pos in positions:
   group=[r for r in samples if r['position']==pos]
   ratio=[]
   for r in group:
    base=next(b for b in rows if b['width']==w and b['position']==pos and b['rep']==r['rep'] and b['variant']=='baseline')
    ratio.append(nps(r)/nps(base))
   if group:perpos.append({'position':pos,'nps_ratio':geo(ratio),'depths':[r['result']['depth'] for r in group],'nps':statistics.mean(nps(r) for r in group),'rss_mib':max(r['peak_rss_kib'] for r in group)/1024})
   paired.extend(ratio)
  m=next((r for r in micro if r['width']==w and r['variant']==variant),None)
  bm=next((r for r in micro if r['width']==w and r['variant']=='baseline'),None)
  micros={k:{'ns':statistics.median(x[k] for x in m['result']['micro']),'speedup':statistics.median(x[k] for x in bm['result']['micro'])/statistics.median(x[k] for x in m['result']['micro'])} for k in ['forward_ns','change_pair_ns','make_unmake_ns']} if m else None
  fixed_ratios=[]
  for v in verify:
   if v['width']!=w or v['variant']!=variant:continue
   bv=next(b for b in verify if b['width']==w and b['position']==v['position'] and b['variant']=='baseline')
   fixed_ratios.append(bv['result']['elapsed_ns']/v['result']['elapsed_ns'])
  summary.append({'width':w,'variant':variant,'relative_nps':geo(paired),'positions':perpos,'fixed_speedup':geo(fixed_ratios) if fixed_ratios else None,'micro':micros})
result={'timed_runs':len(rows),'fixed_runs':len(verify),'summary':summary,'max_elapsed_ms':max(r['result']['elapsed_ns']/1e6 for r in rows),'all_legal':all(r['result']['legal'] for r in rows),'source_directory':str(out)}
hc_path=out/'handcrafted/timed.jsonl'
if hc_path.exists():
 hc=[json.loads(x) for x in hc_path.read_text().splitlines()]
 result['handcrafted_native_speedup']=geo([nps(x)/nps(next(y for y in hc if y['variant']=='baseline' and y['position']==x['position'] and y['rep']==x['rep'])) for x in hc if x['variant']=='native'])
 result['handcrafted_comparison']=[]
 for w in widths:
  for v,hv in [('baseline','baseline'),('native-both','native')]:
   ratio=geo([nps(x)/nps(next(y for y in hc if y['variant']==hv and y['position']==x['position'] and y['rep']==x['rep'])) for x in rows if x['width']==w and x['variant']==v])
   result['handcrafted_comparison'].append({'width':w,'variant':v,'handcrafted_variant':hv,'relative_nps':ratio})
deep_path=out/'depth2/verify.jsonl'
if deep_path.exists(): result['depth2_verified_runs']=len(deep_path.read_text().splitlines())
(out/'summary.json').write_text(json.dumps(result,indent=2))
for r in summary:print(r['width'],r['variant'],round(r['relative_nps'],3),'depths',[p['depths'] for p in r['positions']],'fixed',round(r['fixed_speedup'],2))
