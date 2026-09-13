#!/usr/bin/env python3
"""Summarize small paired blocks; incomplete work never gets a depth speed ratio."""
import gzip,json,math,statistics
from pathlib import Path
from collections import defaultdict
P=Path(__file__).parent/'pilot'
rows=[json.loads(s) for s in gzip.open(P/'raw.jsonl.gz','rt')]
variants=['swap','mobility','tt-first','tt-promote','pvs','asp100','asp500','asp2000','tt-hints','tt-bounds','clock8','clock32','clock128']

def mean_ratio(values):return math.exp(statistics.mean(math.log(x) for x in values)) if values else None

def phase_summary(phase,variant,baseline='combined'):
    subset=[r for r in rows if r['phase']==phase and r['variant'] in [variant,baseline]]
    groups=defaultdict(dict)
    for r in subset:groups[(r['meta']['index'],r['repeat'])][r['variant']]=r
    pairs=[];invalid=[]
    fixed=phase in ['depth2','depth3','baseline']
    for (case,repeat),g in sorted(groups.items()):
        if baseline not in g or variant not in g:continue
        b,v=g[baseline],g[variant]
        if b.get('error') or v.get('error') or (fixed and (b.get('aborted') or v.get('aborted') or b['completed_depth']!=v['completed_depth'])):
            invalid.append(dict(case=case,repeat=repeat));continue
        pairs.append(dict(case=case,repeat=repeat,ratio=v['ms']/b['ms'],baseline_ms=b['ms'],variant_ms=v['ms'],
            baseline_depth=b['completed_depth'],variant_depth=v['completed_depth'],baseline_score=b['score'],variant_score=v['score'],
            baseline_nodes=b['nodes'],variant_nodes=v['nodes'],baseline_best=b['best'],variant_best=v['best'],
            move_changed=b['best']!=v['best'],score_changed=b['score']!=v['score'],nodes_changed=b['nodes']!=v['nodes'],
            lines_changed=b['root_lines']!=v['root_lines'],node_ratio=v['nodes']/max(1,b['nodes']),rss_ratio=v['peak_rss_kib']/b['peak_rss_kib'],
            overshoot_ms=max(0,v['ms']-v['budget_ms']) if v['budget_ms'] else None))
    # Equal position weighting even if a repetition is missing.
    per_case={}
    for case in sorted(set(p['case'] for p in pairs)):
        ps=[p for p in pairs if p['case']==case]
        ratio=statistics.median(p['ratio'] for p in ps)
        per_case[str(case)]=dict(time_saving_pct=100*(1-ratio),pairs=len(ps),
            baseline_ms=statistics.median(p['baseline_ms'] for p in ps),variant_ms=statistics.median(p['variant_ms'] for p in ps),
            node_ratio=statistics.median(p['node_ratio'] for p in ps))
    ratios=[1-c['time_saving_pct']/100 for c in per_case.values()]
    own=[r for r in subset if r['variant']==variant and not r.get('error')]
    counters={}
    for r in own:
        for k,v in r.get('counters',{}).items():counters[k]=counters.get(k,0)+v
    return dict(pairs=len(pairs),invalid=invalid,
        time_saving_pct=100*(1-mean_ratio(ratios)) if ratios else None,
        total_time_saving_pct=100*(1-sum(c['variant_ms'] for c in per_case.values())/sum(c['baseline_ms'] for c in per_case.values())) if ratios else None,
        node_gain_pct=100*(mean_ratio([c['node_ratio'] for c in per_case.values()])-1) if pairs else None,
        changed_moves=sum(p['move_changed'] for p in pairs),changed_move_cases=sorted(set(p['case'] for p in pairs if p['move_changed'])),
        changed_scores=sum(p['score_changed'] for p in pairs),changed_lines=sum(p['lines_changed'] for p in pairs),changed_nodes=sum(p['nodes_changed'] for p in pairs),
        deeper=sum(p['variant_depth']>p['baseline_depth'] for p in pairs),shallower=sum(p['variant_depth']<p['baseline_depth'] for p in pairs),
        max_overshoot_ms=max((p['overshoot_ms'] for p in pairs if p['overshoot_ms'] is not None),default=None),
        peak_rss_change_pct=100*(max((p['rss_ratio'] for p in pairs),default=1)-1),
        counters=counters,per_case=per_case,details=pairs)

report=dict(rows=len(rows),failures=[r for r in rows if r.get('error')],
    mechanical={v:phase_summary('baseline',v,'stock') for v in ['production','combined']},
    memory_check={v:phase_summary('memory_check',v,'stock') for v in ['combined','507']},
    experiments={v:{p:phase_summary(p,v) for p in ['depth2','depth3','timed1s','cross_turn']} for v in variants})
(P/'results.json').write_text(json.dumps(report,indent=2)+'\n')
for name,result in report['mechanical'].items():print('mechanical',name,round(result['time_saving_pct'],1),round(result['total_time_saving_pct'],1))
for name,phases in report['experiments'].items():
 a,b,t,w=(phases[p] for p in ['depth2','depth3','timed1s','cross_turn'])
 def rnd(v):return round(v,1) if v is not None else None
 print(json.dumps(dict(variant=name,d2=rnd(a['time_saving_pct']),d3=rnd(b['time_saving_pct']),
    moves_d2=a['changed_moves'],moves_d3=b['changed_moves'],timed_nodes=rnd(t['node_gain_pct']),
    timed_moves=t['changed_moves'],deeper=t['deeper'],shallower=t['shallower'],
    overshoot=rnd(t['max_overshoot_ms']),warm_nodes=rnd(w['node_gain_pct']),warm_depth=[w['deeper'],w['shallower']],
    warm_counters=w['counters'],d3_counters=b['counters'])))
