"""Package the completed experiment's evidence and human-readable findings."""
import datetime,gzip,hashlib,json,math,pathlib,statistics,subprocess
HERE=pathlib.Path(__file__).resolve().parent;ROOT=HERE.parents[1];OUT=ROOT/'data/derived/search-speed-20260920'
def main():
 assert (OUT/'confirm-finished.json').exists(),'confirmation is unfinished'
 selection=json.loads((OUT/'selection.json').read_text());variant=selection['selected']
 summary=json.loads((OUT/'confirm-summary.json').read_text())[variant]
 corpus=json.loads((OUT/'corpus.json').read_text());plan=json.loads((OUT/'confirm-plan.json').read_text())
 raw=OUT/f'confirm-{variant}.jsonl';rows=[json.loads(x) for x in raw.read_text().splitlines()]
 assert len(rows)==len(plan['cases'])*plan['repeats']
 assert all(r['measurements']['stock']['signature']==r['measurements'][variant]['signature'] for r in rows)
 blocks={}
 for block in [0,1]:
  selected=[r for r in rows if r['rep']//8==block and r['position']>=4]
  blocks[str(block)]=math.exp(statistics.mean(math.log(r['measurements']['stock']['ms']/r['measurements'][variant]['ms']) for r in selected))
 memory=[(r['measurements'][variant]['rss_kib']-r['measurements']['stock']['rss_kib'])/1024 for r in rows if r['measurements']['stock'].get('rss_kib')]
 metadata=dict(created=datetime.datetime.now(datetime.timezone.utc).isoformat(),selected=variant,base=corpus['revision'],
  machine=subprocess.check_output(['lscpu'],text=True),summary=summary,confirmation_plan=plan,process_block_speedup=blocks,
  exact_search_matches=len(rows),mismatches=0,median_resident_delta_mib=statistics.median(memory) if memory else None,
  primary='heldout',all_timed_searches=sum(2*len(p.read_text().splitlines()) for label in ['screen','refinement','confirm'] for p in OUT.glob(label+'-*.jsonl')),
  tests=dict(debug_passed=366,release_passed=366,ignored_each=4,analysis_passed=3))
 results=HERE/'results';results.mkdir(exist_ok=True)
 for label in ['screen','refinement','confirm']:
  (results/f'{label}-summary.json').write_bytes((OUT/f'{label}-summary.json').read_bytes())
  for suffix in ['plan','finished']:
   (results/f'{label}-{suffix}.json').write_bytes((OUT/f'{label}-{suffix}.json').read_bytes())
  for p in OUT.glob(label+'-*.jsonl'):
   # Reproducible gzip header. Raw timings and fingerprints are small enough to retain in Git.
   (results/(p.name+'.gz')).write_bytes(gzip.compress(p.read_bytes(),mtime=0))
 (results/'experiment.json').write_text(json.dumps(metadata,indent=2)+'\n')
 (results/'inputs.json').write_bytes((OUT/'corpus.json').read_bytes())
 (results/'selection.json').write_bytes((OUT/'selection.json').read_bytes())
 (results/'selection-rule.json').write_bytes((OUT/'selection-rule.json').read_bytes())
 (results/'build.json').write_bytes((OUT/f'build-{variant}.json').read_bytes())
 (results/'stock-build.json').write_bytes((OUT/'build-stock.json').read_bytes())
 (results/'candidate.patch').write_bytes((OUT/f'{variant}.patch').read_bytes())
 for label in ['debug','release','analysis']:
  (results/f'tests-{label}.log').write_bytes((OUT/f'tests-{label}.log').read_bytes())
 def fmt(s):return f"{s['speedup']:.3f}× speedup (95% position-cluster bootstrap interval {s['ci95'][0]:.3f}–{s['ci95'][1]:.3f}×; exact two-sided sign-flip p={s['p_two_sided']:.5f})"
 h=summary['heldout']; lines=['# Search speed experiment results','',f"Selected candidate: `{variant}`. Baseline: `{corpus['revision']}`.",'',
  '## Confirmation', '',f"Primary result, eight held-out positions and four agents: **{fmt(h)}**. This corresponds to **{h['time_reduction_pct']:.1f}% less elapsed search time**.",'',
  f"The held-out subset contains {h['pairs']} randomized pairs ({h['pairs']*2} timed searches). The complete confirmation contains {len(rows)} pairs across twelve positions. Four positions were used for tuning; eight were kept out of candidate selection. Every position/agent received sixteen pairs. Both processes were restarted and rewarmed after eight repetitions.",'',
  f"Handcrafted family (secondary): {fmt(summary['heldout_handcrafted'])}.",f"NNUE family (secondary): {fmt(summary['heldout_nnue'])}.",'',
  'Per-agent held-out results (secondary; p-values are not multiplicity adjusted):','']
 for i,m in enumerate(corpus['models']):lines.append(f"- `{m['name']}`: {fmt(summary['heldout_agents'][str(i)])}.")
 lines+=['',f"The position-level mean improved on all eight held-out games, ranging from {min(h['per_position'].values()):.3f}× to {max(h['per_position'].values()):.3f}×. The largest gain was in champs-midgame; late-royal and late-endgame gained about 3%. These differences show why a single opening benchmark would be inadequate."]
 lines+=['','## What worked and what did not','',
  '- Allocation-free direction/ray iteration removed temporary vectors while preserving enumeration order and every path square.',
  '- Quiescence ordering now counts enemy royals once after filtering, derives last-royal capture status from already-computed capture metadata, and performs no such work for a singleton list. The original comparator repeatedly scanned the opposing army.',
  '- Table storage is reused but all written slots are reset after each search. No transposition bounds survive into another search or model. Fully clearing the entire table was slower in tuning; clearing written slots improved the result.',
  '- Thread-local move buffers were slower in the first pass. Moving the pool into the search context and varying initial reservation between 0 and 256 moves also reduced the combined gain. These changes were excluded.',
  '- The handcrafted evaluator benefited more than NNUE. That is consistent with NNUE inference consuming a larger share of total time; these changes optimize shared search work, not the neural-network arithmetic. This interpretation is not an exclusive-time profiling measurement.','',
  '## Validity and limits','',
  f"- {metadata['all_timed_searches']:,} timed searches across screening, refinement and confirmation, including an identical-binary A/A control. The control measured approximately 1.005×, much smaller than the selected effect.",
  f"- All {len(rows)} confirmation pairs matched full best routes, scores, main/q node counts, static scores, completed depths, and ordered root-line fingerprints. Chosen routes were checked against the complete legal move list.",
  f"- Held-out speedup by independent process block: first eight repetitions {blocks['0']:.3f}×; second eight {blocks['1']:.3f}×.",
  f"- CPU-time cross-check on held-out positions: {fmt(summary['heldout_cpu'])}.",
  '- Fixed depth ceilings were chosen using stock-only one-second calibration, capped at depth 3. Comparisons used no deadline and identical depth/settings within each pair. Models and full game histories were loaded outside search timing; fresh-search setup and TT cleanup remained inside timing.',
  '- Timing order was randomized and balanced on pinned CPU 2. No builds ran during measurement. Confidence intervals resample positions, not individual repeated timings; the exact sign-flip test likewise uses position clusters.',
  '- The primary statistical test was specified before confirmation and uses only held-out positions. The overall/tuning and per-agent/family summaries are secondary. Twelve chosen positions on one Intel i7-1255U are not a random sample of every possible game or CPU.',
  '- This is evidence of faster equivalent fixed-depth search, not an Elo result or a guarantee of identical moves under a time limit. The tested evaluators were two handcrafted checkpoints and NNUE widths 512 and 2048; intermediate NNUE widths were not tested.',
  '- Reusable TT buffers retain allocations between searches. Resident-memory deltas are recorded in experiment.json; allocator behavior means retained allocation size is not the same as incremental RSS.',
  f"- Observed median candidate-minus-stock resident memory was {metadata['median_resident_delta_mib']:.1f} MiB across confirmation pairs. Cold-start, densely filled tables at greater depth, and concurrent tournament workers were not measured.",
  '- Correctness: 366 library tests passed in both debug and release, with four existing ignored tests in each profile. Three analysis-method tests passed. New coverage compares all 1,679,616 square pairs and all 256 direction masks with the previous APIs and checks TT clearing across cluster widths.','',
  '## Reproduction and evidence','',
  'See the parent README for commands and protocol. This directory contains compressed raw paired observations, all phase summaries, input identities, compiler/binary/source provenance, selection details, test logs, and the measured source patch. Game records and trained NNUE blobs remain in the ignored local output directory; inputs.json records their original locations and hashes.','']
 (results/'README.md').write_text('\n'.join(lines))
 print(json.dumps({'selected':variant,'primary':h,'handcrafted':summary['heldout_handcrafted'],'nnue':summary['heldout_nnue'],'matches':len(rows),'blocks':blocks},indent=2))
if __name__=='__main__':main()
