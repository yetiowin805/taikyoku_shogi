#!/usr/bin/env python3
"""Derive report figures from raw rows; no engine runs or input changes."""
import collections,json,pathlib,statistics
from analyze import read,EXACT
ROOT=pathlib.Path(__file__).resolve().parent
profiles=read('profile');wall=sum(x['ms'] for x in profiles)
profile_summary={}
for scope,keys in [('diag',['q_generation_ns','q_annotation_ns','q_sort_ns','q_eval_ns']),
 ('profile',['make_ns','eval_ns','atk_ns','gen_ns','order_ns'])]:
 for k in keys:
  values=[x[scope][k]/x['ms']/1e4 for x in profiles]
  profile_summary[k]={'min_percent':min(values),'max_percent':max(values),
   'wall_weighted_percent':sum(x[scope][k] for x in profiles)/wall/1e4}
confirm=read('confirm');control=collections.defaultdict(list)
for x in confirm:
 if x['variant']=='base':control[x['position']].append(x['cpu_ms'])
spread=statistics.median(max(v)/min(v) for v in control.values())
base=[x for x in read('pvs_deep') if x['variant']=='base']
nonleaf=sum(x['diag']['main_nonleaf_nodes'] for x in base);wide=sum(x['diag']['main_wide_nodes'] for x in base)
timed=read('timed');by_variant=collections.defaultdict(list)
for x in timed:by_variant[x['variant']].append(x)
timed_summary={v:{'rows':len(xs),'depth_counts':dict(collections.Counter(x['completed_depth'] for x in xs)),
 'mean_wall_ms':statistics.mean(x['ms'] for x in xs),'mean_cpu_ms':statistics.mean(x['cpu_ms'] for x in xs),
 'min_wall_ms':min(x['ms'] for x in xs),'max_wall_ms':max(x['ms'] for x in xs),
 'abort_count':sum(x['aborted'] for x in xs)} for v,xs in by_variant.items()}
timed_repeats=[]
for v in by_variant:
 a={x['position']:x for x in timed if x['variant']==v and x['repeat']==0}
 b={x['position']:x for x in timed if x['variant']==v and x['repeat']==1}
 ps=sorted(a.keys()&b.keys())
 timed_repeats.append({'variant':v,'positions':len(ps),'depth_changed':sum(a[p]['completed_depth']!=b[p]['completed_depth'] for p in ps),
  'move_changed':sum(a[p]['best']!=b[p]['best'] for p in ps)})
power={s:{'ac':dict(collections.Counter(x[s]['readings'].get('/sys/class/power_supply/AC/online') for x in timed)),
 'epp':dict(collections.Counter(x[s]['readings'].get('/sys/devices/system/cpu/cpu2/cpufreq/energy_performance_preference') for x in timed))} for s in ['power_before','power_after']}
cases=[]
base={(x['position'],x['repeat']):x for x in timed if x['variant']=='base'}
for x in timed:
 b=base.get((x['position'],x['repeat']))
 if b and x['variant']!='base' and (x['best']!=b['best'] or x['completed_depth']!=b['completed_depth']):
  cases.append({'position':x['position'],'name':x['name'],'repeat':x['repeat'],'variant':x['variant'],
   'control_depth':b['completed_depth'],'candidate_depth':x['completed_depth'],
   'move_changed':b['best']!=x['best'],'control_score':b['score'],'candidate_score':x['score']})
result={'profile':profile_summary,'confirm_control_median_max_min_cpu_ratio':spread,
 'confirm_rss_mib_range':[min(x['peak_rss_kib'] for x in confirm)/1024,max(x['peak_rss_kib'] for x in confirm)/1024],
 'qfast_early_rejected':sum(x['diag']['q_early_rejected'] for x in confirm if x['variant']=='qfast'),
 'qfast_raw_candidates':sum(x['diag']['q_raw'] for x in confirm if x['variant']=='qfast'),
 'nonleaf_window_calls':{'total':nonleaf,'wide':wide,'narrow_percent':100*(nonleaf-wide)/nonleaf},
 'timed':timed_summary,'timed_repeat_stability':timed_repeats,'timed_power':power,'timed_disagreements':cases}
(ROOT/'results/report-evidence.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
