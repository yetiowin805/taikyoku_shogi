#!/usr/bin/env python3
"""Paired summaries with position-level bootstrap intervals and full outcomes."""
import gzip, json, math, random, statistics, sys
from collections import defaultdict
from pathlib import Path


def geometric(values):
    return math.exp(statistics.mean(map(math.log, values))) if values else None


def summarize(root):
    rows=[]
    for path in sorted(root.glob('raw-cpu*.jsonl*')):
        opener=gzip.open if path.suffix=='.gz' else open
        with opener(path,'rt') as f:rows.extend(json.loads(line) for line in f)
    indexed={(r['phase'],r['meta']['index'],r['repeat'],r['variant']):r for r in rows}
    assert len(indexed)==len(rows),'duplicate observations: use a fresh output directory'
    failures=[r for r in rows if r.get('error')]
    good=[r for r in rows if not r.get('error')]
    rng=random.Random(20260913)

    def comparison(phase,variant,baseline):
        pairs=[]
        for v in good:
            if v['phase']!=phase or v['variant']!=variant:continue
            b=indexed.get((phase,v['meta']['index'],v['repeat'],baseline))
            if not b or b.get('error'):continue
            assert b['cpu']==v['cpu']
            if phase=='fixed2' and (b['completed_depth']!=2 or v['completed_depth']!=2 or b['aborted'] or v['aborted']):continue
            pairs.append(dict(case=v['meta']['index'],name=v['meta']['name'],group=v['meta']['group'],repeat=v['repeat'],
                baseline_ms=b['ms'],variant_ms=v['ms'],baseline_nodes=b['nodes'],variant_nodes=v['nodes'],
                nps_ratio=(v['nodes']/v['ms'])/(b['nodes']/b['ms']) if b['nodes']>0 and v['nodes']>0 else None,
                elapsed_ratio=v['ms']/b['ms'],rss_ratio=v['peak_rss_kib']/b['peak_rss_kib'],
                baseline_depth=b['completed_depth'],variant_depth=v['completed_depth'],
                baseline_score=b['score'],variant_score=v['score'],baseline_best=b['best'],variant_best=v['best'],
                move_changed=b['best']!=v['best'],score_changed=b['score']!=v['score'],
                nodes_changed=b['nodes']!=v['nodes'],root_lines_changed=b['root_lines']!=v['root_lines'],
                overshoot_ms=max(0,v['ms']-v['budget_ms']) if v['budget_ms'] else None,
                ended_early=v['budget_ms']>0 and v['ms']<v['budget_ms']*.9))
        positions=[]
        for case in sorted({p['case'] for p in pairs}):
            ps=[p for p in pairs if p['case']==case]
            ratios=[p['nps_ratio'] for p in ps if p['nps_ratio'] is not None]
            positions.append(dict(case=case,name=ps[0]['name'],group=ps[0]['group'],pairs=len(ps),
                nps_ratio=statistics.median(ratios) if ratios else None,
                elapsed_ratio=statistics.median(p['elapsed_ratio'] for p in ps),
                peak_rss_ratio=max(p['rss_ratio'] for p in ps)))
        ratios=[p['nps_ratio'] for p in positions if p['nps_ratio'] is not None]
        bootstrap=sorted(100*(geometric(rng.choices(ratios,k=len(ratios)))-1) for _ in range(2000)) if ratios else []
        counters=defaultdict(int)
        own=[r for r in good if r['phase']==phase and r['variant']==variant]
        for r in own:
            for k,v in r.get('counters',{}).items():counters[k]+=v
        groups={}
        for group in sorted({p['group'] for p in positions}):
            gp=[p for p in positions if p['group']==group];gr=[p['nps_ratio'] for p in gp if p['nps_ratio'] is not None]
            groups[group]=dict(positions=len(gp),nps_change_pct=100*(geometric(gr)-1) if gr else None,
                elapsed_saving_pct=100*(1-geometric([p['elapsed_ratio'] for p in gp])))
        return dict(pairs=len(pairs),positions=len(positions),
            nps_change_pct=100*(geometric(ratios)-1) if ratios else None,
            nps_bootstrap95_pct=[bootstrap[49],bootstrap[1949]] if bootstrap else None,
            elapsed_saving_pct=100*(1-geometric([p['elapsed_ratio'] for p in positions])) if positions else None,
            total_elapsed_saving_pct=100*(1-sum(p['variant_ms'] for p in pairs)/sum(p['baseline_ms'] for p in pairs)) if pairs else None,
            deeper=sum(p['variant_depth']>p['baseline_depth'] for p in pairs),shallower=sum(p['variant_depth']<p['baseline_depth'] for p in pairs),
            changed_moves=sum(p['move_changed'] for p in pairs),changed_scores=sum(p['score_changed'] for p in pairs),
            changed_nodes=sum(p['nodes_changed'] for p in pairs),changed_lines=sum(p['root_lines_changed'] for p in pairs),
            early_searches=sum(p['ended_early'] for p in pairs),
            max_overshoot_ms=max((p['overshoot_ms'] for p in pairs if p['overshoot_ms'] is not None),default=None),
            max_peak_rss_change_pct=100*(max((p['rss_ratio'] for p in pairs),default=1)-1),
            counters=dict(counters),groups=groups,per_position=positions,details=pairs)

    variability={}
    for variant in sorted({r['variant'] for r in good if r['phase']=='timed3s'}):
        sets=defaultdict(list)
        for r in good:
            if r['phase']=='timed3s' and r['variant']==variant:sets[r['meta']['index']].append(r)
        variability[variant]=dict(positions=len(sets),
            variable_moves=sum(len({json.dumps(r['best'],sort_keys=True) for r in rs})>1 for rs in sets.values()),
            variable_depth=sum(len({r['completed_depth'] for r in rs})>1 for rs in sets.values()),
            variable_scores=sum(len({r['score'] for r in rs})>1 for rs in sets.values()))
    report=dict(rows=len(rows),failures=failures,
        fixed_mechanical=comparison('fixed2','combined','stock'),
        mechanical=comparison('timed3s','combined','stock'),
        variants={v:comparison('timed3s',v,'combined') for v in sorted({r['variant'] for r in good}-{'stock','combined'})},
        warm_hints=comparison('warm3s','tt-hints','combined'),repeat_variability=variability,
        caveat='NPS is throughput, not playing strength. Bootstrap samples positions; shared games reduce independence. Per-position ratios are medians of paired repetitions.')
    (root/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
    print('rows',len(rows),'failures',len(failures))
    for v,r in [('mechanical',report['mechanical']),*report['variants'].items(),('warm_hints',report['warm_hints'])]:
        print(v,json.dumps({k:r[k] for k in ('pairs','nps_change_pct','nps_bootstrap95_pct','deeper','shallower','changed_moves','changed_scores','max_overshoot_ms')}))
    return report


if __name__=='__main__': summarize(Path(sys.argv[1]))
