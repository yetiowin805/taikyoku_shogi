"""Paired log ratios, treating positions as clusters rather than repetitions as new positions."""
import argparse,itertools,json,math,pathlib,random,statistics
OUT=pathlib.Path(__file__).resolve().parents[2]/'data/derived/search-speed-20260920'
def summarize(rows,variant,model=None,positions=None,metric='ms'):
 groups={}
 for row in rows:
  if model is not None and row['model'] not in model:continue
  if positions is not None and row['position'] not in positions:continue
  a=row['measurements']['stock'][metric];b=row['measurements'][variant][metric]
  groups.setdefault((row['position'],row['model']),[]).append(math.log(a/b))
 cells={k:statistics.mean(v) for k,v in groups.items()};clusters={}
 for (pos,agent),value in cells.items():clusters.setdefault(pos,[]).append(value)
 cluster=[statistics.mean(v) for v in clusters.values()];mu=statistics.mean(cluster)
 rng=random.Random(241107);boots=sorted(statistics.mean(rng.choices(cluster,k=len(cluster))) for _ in range(20000))
 p=sum(abs(statistics.mean(s*x for s,x in zip(signs,cluster)))>=abs(mu)-1e-14 for signs in itertools.product([-1,1],repeat=len(cluster)))/2**len(cluster)
 return dict(speedup=math.exp(mu),percent_faster=(math.exp(mu)-1)*100,time_reduction_pct=(1-math.exp(-mu))*100,
  ci95=[math.exp(boots[500]),math.exp(boots[19499])],p_two_sided=p,positions=len(cluster),cells=len(cells),pairs=sum(map(len,groups.values())),
  per_position={str(pos):math.exp(statistics.mean(v)) for pos,v in clusters.items()})
def main():
 p=argparse.ArgumentParser();p.add_argument('label');args=p.parse_args();result={}
 for path in sorted(OUT.glob(args.label+'-*.jsonl')):
  variant=path.name[len(args.label)+1:-6];rows=[json.loads(x) for x in path.read_text().splitlines()]
  if not rows:continue
  result[variant]={'overall':summarize(rows,variant),'handcrafted':summarize(rows,variant,[0,1]),'nnue':summarize(rows,variant,[2,3]),'cpu':summarize(rows,variant,metric='cpu_ms'),
   'agents':{str(m):summarize(rows,variant,[m]) for m in range(4)}}
  if any(r['position']>=4 for r in rows):
   held=sorted({r['position'] for r in rows if r['position']>=4})
   result[variant]['heldout']=summarize(rows,variant,positions=held)
   result[variant]['heldout_handcrafted']=summarize(rows,variant,[0,1],positions=held)
   result[variant]['heldout_nnue']=summarize(rows,variant,[2,3],positions=held)
   result[variant]['heldout_agents']={str(m):summarize(rows,variant,[m],positions=held) for m in range(4)}
   result[variant]['heldout_cpu']=summarize(rows,variant,positions=held,metric='cpu_ms')
  print(variant, json.dumps({k:v for k,v in result[variant].items() if k in ['overall','handcrafted','nnue','heldout']}),flush=True)
 (OUT/f'{args.label}-summary.json').write_text(json.dumps(result,indent=2)+'\n')
if __name__=='__main__':main()
