#!/usr/bin/env python3
"""Summarize paired speed probes separately from evaluation-quality ablations."""
import json,math,pathlib,statistics,sys
out=pathlib.Path(sys.argv[1])
def load(name):
 p=out/(name+'.jsonl');return [json.loads(s) for s in p.read_text().splitlines()] if p.exists() else []
def geo(v):return math.exp(statistics.mean(math.log(x) for x in v))
def nps(x):return x['result']['nodes']/(x['result']['elapsed_ns']/1e9)
rows=load('timed');verify=load('verify');quality=load('quality-search');micro=load('micro');counters=load('counters')
summary={'timed_runs':len(rows),'fixed_depth_runs':len(verify),'quality_search_runs':len(quality),'speed':[],'quality_search':[],'micro':[],'counters':[]}
for w in sorted({r['width'] for r in rows}):
 for v in dict.fromkeys(r['variant'] for r in rows):
  group=[r for r in rows if r['width']==w and r['variant']==v]
  if not group:continue
  control='exact' if v.startswith('head') else 'control';paired=[];positions=[]
  for pos in dict.fromkeys(r['position'] for r in group):
   g=[r for r in group if r['position']==pos];ratios=[];vs=[]
   for r in g:
    b=next(b for b in rows if b['width']==w and b['position']==pos and b['rep']==r['rep'] and b['variant']==control)
    ratios.append(nps(r)/nps(b));paired.append(nps(r)/nps(b));vs.append(r['result']['depth'])
   positions.append({'position':pos,'nps_ratio':geo(ratios),'paired_ratios':ratios,'depths':vs,'scores':[r['result']['score'] for r in g]})
  references=[next(b for b in rows if b['width']==w and b['position']==r['position'] and b['rep']==r['rep'] and b['variant']==control) for r in group]
  pooled=(sum(r['result']['nodes'] for r in group)/sum(r['result']['elapsed_ns'] for r in group))/(sum(r['result']['nodes'] for r in references)/sum(r['result']['elapsed_ns'] for r in references))
  summary['speed'].append({'pooled_nps_ratio':pooled,'width':w,'variant':v,'reference':control,'nps_ratio':geo(paired),'positions':positions,'peak_rss_mib':max(r['peak_rss_kib'] for r in group)/1024,'max_search_ms':max(r['result']['elapsed_ns'] for r in group)/1e6})
for w in sorted({r['width'] for r in verify}):
 for v in ('head24','head16'):
  group=[r for r in verify+quality if r['width']==w and r['variant']==v];details=[]
  for r in group:
   b=next(b for b in verify+quality if b['width']==w and b['position']==r['position'] and b['variant']=='exact')
   changed=r['result']['best']!=b['result']['best']
   details.append({'position':r['position'],'game':r['game'],'ply':r['ply'],'depth':r['result']['depth'],'changed':changed,'original_score':b['result']['score'],'variant_score':r['result']['score'],'original_move':b['result']['best'],'variant_move':r['result']['best']})
  if group:summary['quality_search'].append({'width':w,'variant':v,'positions':len(details),'changed_moves':sum(r['changed'] for r in details),'details':details})
for r in micro:
 b=next(b for b in micro if b['width']==r['width'] and b['variant']=='control')
 metrics={k:{'ns':statistics.median(s[k] for s in r['result']['micro']),'speedup':statistics.median(s[k] for s in b['result']['micro'])/statistics.median(s[k] for s in r['result']['micro'])} for k in ('forward_ns','change_pair_ns','make_unmake_ns')}
 summary['micro'].append({'width':r['width'],'variant':r['variant'],'metrics':metrics})
for r in counters:summary['counters'].append({k:r[k] for k in ('width','position','variant')}|{'counters':r['result']['counters'],'nodes':r['result']['nodes']})
summary['quality']=json.loads((out/'quality-summary.json').read_text())
(out/'followup-summary.json').write_text(json.dumps(summary,indent=2))
for r in summary['speed']:print(r['width'],r['variant'],'vs',r['reference'],round(r['nps_ratio'],3),'positions',[round(x['nps_ratio'],3) for x in r['positions']])
for r in summary['quality_search']:print('quality',r['width'],r['variant'],'changed',r['changed_moves'],'/',r['positions'])
