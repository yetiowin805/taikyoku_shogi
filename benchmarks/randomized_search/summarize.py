#!/usr/bin/env python3
"""Stream paired measurements; equal-agent summaries keep sample imbalance bounded."""
import collections,gzip,json,math,statistics,sys
from pathlib import Path
BASELINE='S0-W0-T0'

def aggregate(pairs):
    per_agent={}
    for agent in sorted({p['agent'] for p in pairs}):
        ps=[p for p in pairs if p['agent']==agent]
        per_agent[agent]=dict(pairs=len(ps),nps_change_pct=100*(math.exp(statistics.mean(p['log_nps_ratio'] for p in ps))-1),
            deeper=sum(p['depth_delta']>0 for p in ps),shallower=sum(p['depth_delta']<0 for p in ps),changed_moves=sum(p['move_changed'] for p in ps))
    value=100*(math.exp(statistics.mean(math.log1p(r['nps_change_pct']/100) for r in per_agent.values()))-1) if per_agent else None
    return dict(pairs=len(pairs),equal_agent_nps_change_pct=value,per_agent=per_agent,
        deeper=sum(p['depth_delta']>0 for p in pairs),shallower=sum(p['depth_delta']<0 for p in pairs),
        changed_moves=sum(p['move_changed'] for p in pairs),changed_scores=sum(p['score_changed'] for p in pairs),
        max_overshoot_ms=max((p['overshoot_ms'] for p in pairs),default=0),
        max_rss_ratio=max((p['rss_ratio'] for p in pairs),default=1))

def main(root):
    failures=[];rows=0;direct=collections.defaultdict(list);all_pairs=[];incomplete=[];counts=collections.Counter()
    def paired_rows():
        nonlocal rows
        for path in sorted(root.glob('raw-cpu*.jsonl.gz')):
            previous=None
            with gzip.open(path,'rt') as f:
                for line in f:
                    row=json.loads(line);rows+=1
                    if row.get('error'):failures.append(row)
                    if previous is None:previous=row;continue
                    if previous['pair']!=row['pair']:
                        incomplete.append(previous['pair']);previous=row;continue
                    yield previous,row
                    previous=None
            if previous is not None:incomplete.append(previous['pair'])
    for a,b in paired_rows():
        pair=a['pair']
        if a.get('error') or b.get('error'):incomplete.append(pair);continue
        if a['variant']==b['variant'] or a['case']!=b['case'] or a['agent']!=b['agent'] or a['cpu']!=b['cpu']:raise ValueError('invalid pair')
        if a['meta']['position_hash']!=b['meta']['position_hash']:raise ValueError('position identity mismatch')
        counts[a['agent']]+=1
        if a['variant']!=BASELINE and b['variant']==BASELINE:a,b=b,a
        if min(a['nodes'],b['nodes'])<=0:continue
        entry=dict(pair=pair,case=a['case'],agent=a['agent'],a=a['variant'],b=b['variant'],
            log_nps_ratio=math.log((b['nodes']/b['ms'])/(a['nodes']/a['ms'])),
            depth_delta=b['completed_depth']-a['completed_depth'],move_changed=b['best']!=a['best'],score_changed=b['score']!=a['score'],
            overshoot_ms=max(0,b['ms']-b['budget_ms']),rss_ratio=b['peak_rss_kib']/a['peak_rss_kib'],
            a_depth=a['completed_depth'],b_depth=b['completed_depth'],a_score=a['score'],b_score=b['score'],a_best=a['best'],b_best=b['best'])
        all_pairs.append(entry)
        if a['variant']==BASELINE:direct[b['variant']].append(entry)
    result=dict(rows=rows,complete_pairs=sum(counts.values()),agents=dict(counts),failures=failures,incomplete_pairs=incomplete,
        baseline=BASELINE,configurations={k:aggregate(v) for k,v in sorted(direct.items())},
        note='Direct paired comparisons to mechanical baseline, equally weighted across sampled agents. No playing-strength inference. All random-vs-random pairs retained for interaction modeling.')
    (root/'summary.json').write_text(json.dumps(result,indent=2)+'\n')
    with gzip.open(root/'pairs.jsonl.gz','wt') as f:
        for row in all_pairs:f.write(json.dumps(row,separators=(',',':'))+'\n')
    print(json.dumps(result,indent=2))

if __name__=='__main__':main(Path(sys.argv[1]))
