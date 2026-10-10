#!/usr/bin/env python3
import collections,json,math,pathlib,statistics,random
ROOT=pathlib.Path(__file__).resolve().parent
CORPUS=json.loads((ROOT/'corpus.json').read_text())
def interval(ratios):
 if len(ratios)<3:return None
 groups=collections.defaultdict(list)
 for p,v in ratios.items():groups[CORPUS['positions'][p]['game']].append(math.log(statistics.median(v)))
 blocks=list(groups.values());rng=random.Random(45010);means=[]
 for _ in range(4000):
  values=[v for block in rng.choices(blocks,k=len(blocks)) for v in block]
  means.append(math.exp(statistics.mean(values)))
 means.sort();return [means[100],means[3899]]
EXACT=['best' ,'score','completed_depth','nodes','q_nodes','q_generated','q_searched','q_tt_hits','lines']
def read(group):
 p=ROOT/'results'/f'{group}.jsonl';return [json.loads(l) for l in p.read_text().splitlines()] if p.exists() else []
def summarize(group):
 rows=read(group);base={(x['position'],x['repeat']):x for x in rows if x['variant']=='base'};out=[]
 for variant in sorted({x['variant'] for x in rows}-{'base'}):
  pairs=[(base[x['position'],x['repeat']],x) for x in rows if x['variant']==variant and (x['position'],x['repeat']) in base]
  complete=[(b,x) for b,x in pairs if not b['aborted'] and not x['aborted'] and b['completed_depth']==x['completed_depth']==x['depth']]
  ratios=collections.defaultdict(list)
  for b,x in complete:ratios[x['position']].append(x['cpu_ms']/b['cpu_ms'])
  pos_ratios=[statistics.median(v) for v in ratios.values()]
  out.append(dict(variant=variant,pairs=len(pairs),complete_pairs=len(complete),positions=len(ratios),
   cpu_ratio_interval=interval(ratios),
   cpu_ratio=math.exp(statistics.mean(map(math.log,pos_ratios))) if pos_ratios else None,
   summed_cpu_ratio=sum(x['cpu_ms'] for b,x in complete)/sum(b['cpu_ms'] for b,x in complete) if complete else None,
   min_position_ratio=min(pos_ratios) if pos_ratios else None,max_position_ratio=max(pos_ratios) if pos_ratios else None,
   nodes_ratio=math.exp(statistics.mean(math.log(x['nodes']/b['nodes']) for b,x in complete)) if complete else None,
   exact_mismatches=sum(any(b[k]!=x[k] for k in EXACT) for b,x in complete),
   base_depth_avg=statistics.mean(b['completed_depth'] for b,x in pairs) if pairs else None,
   candidate_depth_avg=statistics.mean(x['completed_depth'] for b,x in pairs) if pairs else None,
   timed_nodes_ratio=math.exp(statistics.mean(math.log(x['nodes']/b['nodes']) for b,x in pairs)) if pairs else None,
   same_depth_move_changes=sum(b['completed_depth']==x['completed_depth'] and b['best']!=x['best'] for b,x in pairs),
   move_changes=sum(b['best']!=x['best'] for b,x in pairs),depth_up=sum(x['completed_depth']>b['completed_depth'] for b,x in pairs),
   depth_down=sum(x['completed_depth']<b['completed_depth'] for b,x in pairs),
   diag={k:sum(x.get('diag',{}).get(k,0) for b,x in pairs) for k in ['q_raw','q_early_rejected','q_royal_probes_skipped','q_sort_calls','q_sort_candidates','pvs_scouts','pvs_depth_researches','pvs_window_researches','pv_tt_score_bypasses','main_nonleaf_nodes','main_wide_nodes','pvs_pv_fail_high_verifications']}))
 return out
if __name__=='__main__':
 result={g:summarize(g) for g in ['screen','confirm','pvs_deep','timed','production'] if read(g)}
 # Retain the interrupted batch as an explicit audit, never a speed comparison.
 stopped=read('holdout');pairs=summarize('holdout')
 result['interrupted_holdout']={'rows':len(stopped),'planned_rows':56,
  'reason':'Stopped after power-state change: AC offline, power preference, CPU2 sampled at 700 MHz. Transition time is unknown. All timings and cap-based depth comparisons excluded.',
  'complete_pairs':sum(x['complete_pairs'] for x in pairs),
  'exact_mismatches_in_complete_pairs':sum(x['exact_mismatches'] for x in pairs)}
 identity=read('identity');screen={x['position']:x for x in read('screen') if x['variant']=='base'}
 result['identity_v1']=[dict(position=b['position'],equal=all(b[k]==screen[b['position']][k] for k in EXACT)) for b in identity if b['position'] in screen]
 final={x['position']:x for x in read('identity_v3')}
 result['identity']=[dict(position=b['position'],equal=all(b[k]==final[b['position']][k] for k in EXACT)) for b in identity if b['position'] in final]
 (ROOT/'results/summary.json').write_text(json.dumps(result,indent=2)+'\n')
 print(json.dumps(result,indent=2))
