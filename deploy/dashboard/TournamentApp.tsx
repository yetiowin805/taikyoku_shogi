import { H1, H2, Text, Stack, Row, Select, useCanvasState, useHostTheme } from "cursor/canvas";
const D = __DASHBOARD_D__;

const R = __DASHBOARD_R__;
const ACTIVE_ONLY = __DASHBOARD_ACTIVE_ONLY__;
const sccColor=(id:number)=>`var(--scc-${id}, hsl(${(id*137.508)%360} 55% 48%))`;
const TABS = [["ratings", "Ratings"], ["scheduler", "Scheduler"], ["graph", "Comparison graph"], ["generations", "Generations"], ["generation-strength", "Generation strength"], ["retirement", "Retirement"], ["archive", "Archive"]];
export default function TournamentTree(){
 const t=useHostTheme(); const [tab,setTab]=useCanvasState("view","ratings"); const [treeId,setTree]=useCanvasState("tree","48"); const [selected,setSelected]=useCanvasState("match",""); const [component,setComponent]=useCanvasState("component",0);
 const view=TABS.some(([id])=>id===tab)?tab:"ratings";
 const tree=D.trees.find(x=>String(x.id)===treeId)||D.trees[D.trees.length-1]; const match=tree?.nodes.find(n=>n.key===selected); const c=D.components.find(x=>x.id===component)||D.components[0];
 const variant=(s:string)=>/^(H|L|N|S|T|Q|HL|ST|QFIX|HLF.*|FN)$/.test(s)||/_HLF$/.test(s);
 return <Stack gap={18} style={{padding:24,color:t.text.primary,background:t.bg.editor,fontFamily:"sans-serif"}}>
 <H1>Tournament dashboard</H1>
 <Text>{D.snapshotLabel} · {D.done.toLocaleString()} completed games · {D.activeCount} active · {D.retiredCount} inactive</Text>
 <nav aria-label="Dashboard views" style={{display:"flex",gap:8,flexWrap:"wrap"}}>{TABS.map(([id,label])=><button key={id} aria-pressed={view===id} onClick={()=>setTab(id)}>{label}</button>)}</nav>
 {view==="ratings"?<RatingsView/>:view==="scheduler"?<SchedulerView h={(Tag,props)=><Tag {...props}/>} data={D.scheduler} theme={t}/>:view==="generations"?<GenerationsView/>:view==="generation-strength"?<GenerationStrengthView h={(Tag,props)=><Tag {...props}/>} theme={t}/>:view==="retirement"?<Text>Current eligibility, exposure and protection from the coordinator:</Text>:view==="archive"?<Stack gap={12}>
 <H2>Historical reports & archived brackets</H2>
 <Text><a href="dashboard-archive-20261112.html">Earlier dashboard, launch proposal and frozen SEEDS2 comparison</a> · <a href="generation-strength-study-20261112.html">Original statistical study and validation</a> · <a href="historical-tournament-20261031.html">Leaderboard before the rules reset</a></Text>
 <details><summary>Active-only rating sensitivity</summary><ActiveOnlyView/></details>
 {tree&&<details><summary>Archived knockout brackets</summary><Stack gap={12}>
 <Row gap={16}><H2>Bracket {tree.id}</H2><Select value={treeId} onChange={v=>{setTree(v);setSelected("")}} options={D.trees.slice().reverse().map(x=>({value:String(x.id),label:`Bracket ${x.id} · ${x.seeds} entrants`}))}/></Row>
 <Text>Blue names mark search variants; bold names won their match. Ratings below names use the latest full-history fit, not the rating at game time. * marks provisional cross-SCC offsets. Scores include completed games only. Dashed boxes show scheduled advancement paths whose participants are still being resolved.</Text>
 <div style={{overflowX:"auto",border:`1px solid ${t.stroke.secondary}`}}><svg width={tree.width} height={tree.height} role="img" aria-label={`Knockout bracket ${tree.id}; click a match for game details`}>
 {[...new Set(tree.nodes.map(n=>n.x))].map(x=><text key={x} x={x+8} y={24} fill={t.text.secondary} fontSize={13}>{x===10?"Play-in":tree.nodes.find(n=>n.x===x)?.label.split('.')[0].replace('R','Round of ')}</text>)}
 {tree.nodes.flatMap(n=>n.prev.map(p=>{const a=tree.nodes.find(v=>v.key===p)!;return <path key={n.key+p} d={`M${a.x+320},${a.y+50} H${a.x+332} V${n.y+50} H${n.x}`} fill="none" stroke={t.stroke.primary}/>;}))}
 {tree.nodes.map(n=><g key={n.key} onClick={()=>setSelected(n.key)} tabIndex={0} onKeyDown={e=>{if(e.key==='Enter')setSelected(n.key)}} style={{cursor:"pointer"}}>
 <rect x={n.x} y={n.y} width={320} height={112} rx={4} fill={selected===n.key?t.fill.secondary:t.bg.elevated} stroke={t.stroke.primary} strokeDasharray={n.created?undefined:"4 4"}/>
 <text x={n.x+9} y={n.y+17} fontSize={11} fill={t.text.tertiary}>{n.label} · {n.winner?"finished":n.running?`${n.running} running`:n.created?"queued":"awaiting feeder winner"}</text>
 {[n.a,n.b].map((name,j)=><text key={j} x={n.x+9} y={n.y+36+j*27} fontSize={12} fill={variant(name)?t.accent.primary:t.text.primary} fontWeight={n.winnerLabel===name?700:400}><title>{agentWithRating(name)}</title>{name}<tspan x={n.x+300} textAnchor="end">{n.done?n.score[j]:""}</tspan><tspan x={n.x+9} y={n.y+49+j*27} fontSize={10} fill={t.text.secondary} fontWeight={400}>{agentRatingText(name)}</tspan></text>)}
 <text x={n.x+9} y={n.y+101} fontSize={10} fill={t.text.tertiary}>{n.created?`${n.done} finished · ${n.pending} queued games`:"Advancement derived from seeded bracket"}</text>
 </g>)}
 </svg></div>
 {match&&<div><H2>{match.label}: {agentWithRating(match.a)} vs {agentWithRating(match.b)}</H2><Text>{match.winnerLabel?`Winner: ${agentWithRating(match.winnerLabel)}. `:"Match unresolved. "}Completed-game points: {match.score.join("–")}. {match.slots.map(g=>`Slot ${g.id}: ${g.status}${g.status==='done'?`, ${g.a} scored ${g.score}`:''}, ${g.start} start`).join("; ")}</Text></div>}
 <Text>These are historical knockout brackets. Under adaptive scheduling, no new brackets or advancement games are created; unresolved paths remain archived. Completed games remain rating evidence.</Text>

 </Stack></details>}
 </Stack>:<Stack gap={14}>
 <H2>{D.weakCount===1?`Connected graph · ${D.components.length} strongly connected component${D.components.length===1?"":"s"}`:`${D.weakCount} disconnected groups · ${D.components.length} strongly connected component${D.components.length===1?"":"s"}`}</H2>
 <Text>{D.weakCount===1?`All ${D.activeCount} active agents now belong to one connected comparison graph. ${D.components.length} strongly connected component${D.components.length===1?"":"s"} remain; links between their colors are one-way.`:`${D.weakCount} groups remain disconnected in completed-game evidence.`} Running games count only after completion. Retired opponents retain historical evidence.</Text>
 <ComparisonFlow h={(Tag,props)=><Tag {...props}/>} data={D} rows={R.rows} theme={t} selected={component} onSelect={setComponent}/>
 <Text>Solid arrows point from scoring agent/group to opponent; labels count points across component boundaries. Each box collapses a strongly connected component: every member has a directed scoring path to every other member. Current assignments are listed in the Scheduler tab. Layout is schematic; position is not a rating.</Text>
 <Text><strong>{c.label} · SCC {c.id+1}:</strong> {c.members.map(name=>{const r=R.rows.find(r=>r.name===name);return `${name}: ${r?.games?r.rating.toFixed(1):"unrated"}`;}).join(", ")}{c.retired?` plus ${c.retired} retired agents`:""}.</Text>

 </Stack>}
 <details><summary>Data & method</summary><Text>Published ratings use the coordinator’s full-history fit. Generation strength is a separate hierarchical estimate. Only completed games count as evidence. <a href="nnue-generation-latest-state.json">Tournament snapshot</a> · <a href="nnue-generation-latest-fit.json">Published fit</a></Text></details>
 </Stack>;
}

function RatingsView(){
 const [filter,setFilter]=useCanvasState("rating-component","all");
 const rows=R.rows.filter(r=>filter==="all"||String(r.scc)===filter);
 return <Stack gap={16}>
 <H2>Ratings by strongly connected component</H2>
 <Text>Same color = the same strongly connected component (SCC): every member has a path of wins or draws back to every other member. {D.components.length===1?"All agents now share one color: the fitted ratings are comparable across the field.":"Different colors in the main connected group have only one-way links between them. Disconnected groups are listed separately."}</Text>
 {R.status==="converged"?<div className="rating-note"><strong>Fit converged · one strongly connected component.</strong> These ratings now come from one joint Bradley–Terry fit, with no artificial gaps between components. Sample sizes vary across variants. Read the W–D–L records alongside the ratings; estimates can still move as results arrive.</div>:R.status==="disconnected"?<div className="rating-note"><strong>New agents are not yet connected.</strong> Each disconnected group is independently centered at 1500. New entrants with no finished games are unrated; their displayed placeholder is not comparable to the established field.</div>:<div className="rating-note"><strong>Provisional does not mean proven stronger.</strong> The estimator imposes a minimum 100-point gap between separated components. Those cross-color gaps are a display convention, not measured strength differences. Compare fitted differences within one color.</div>}

 <label className="rating-filter">Component <select aria-label="Rating component" value={filter} onChange={e=>setFilter(e.target.value)}><option value="all">All active agents</option>{D.components.map(c=><option key={c.id} value={String(c.id)}>SCC {c.id+1}: {c.label}</option>)}</select></label>
 <div className="rating-legend">{D.components.map(c=><span key={c.id}><i style={{background:sccColor(c.id)}}/>SCC {c.id+1} · {c.label}</span>)}</div>
 {Array.from({length:D.weakCount},(_,i)=>i).map(group=>{const members=rows.filter(r=>r.group===group);if(!members.length)return null;return <section key={group}>
 <h2>{group===0?"Main connected group":"Disconnected group · "+members.map(r=>r.name).join(" + ")}</h2>
 <p className="rating-caption">{group===0?`${members.length} active agents, with ${D.retiredCount} retired agents retained as historical evidence. ${R.status==="converged"?"Sorted by fitted rating. All ratings share the same comparison scale.":"Sorted by displayed rating; cross-color ordering is provisional."}`:members.every(r=>r.games===0)?"No completed games. 1500 is an independent centering placeholder, not a comparison with the main group.":"Completed games connect these agents to each other. This group is independently centered at 1500 and cannot yet be compared with the main group."}</p>
 <div className="rating-table-wrap"><table className="rating-table"><thead><tr><th>Agent</th><th>Rating</th><th>SCC</th><th>W–D–L</th><th>Evidence</th></tr></thead><tbody>{members.map(r=><tr key={r.id} style={{background:`color-mix(in srgb, ${sccColor(r.scc)} 7%, transparent)`}}><td><span className="rating-dot" style={{background:sccColor(r.scc)}}/>{r.name}</td><td className="rating-number">{r.rating.toFixed(1)}{r.games===0?"*":""}</td><td><span className="rating-component" style={{color:sccColor(r.scc)}}>SCC {r.scc+1}</span></td><td className="rating-number">{r.w}–{r.d}–{r.l}</td><td>{r.games===0?"Unrated":R.status==="disconnected"&&group!==0?"Group rating":r.scc===D.coreScc?"Fitted rating":"Provisional offset"}</td></tr>)}</tbody></table></div>
 </section>})}
 <Text>These are the coordinator’s order-neutral Bradley–Terry ratings, not the knockout Glicko seed ratings. Active agents center at 1500 separately in each disconnected group. Absolute levels can shift as groups connect; within-SCC fitted differences are preserved by the current embedding. Records include all completed games in this run, including supplemental games.</Text>
 <Text>Snapshot: {D.snapshotLabel}. Computed locally from the saved state using the live coordinator’s rating policy: {D.liveRetirementPolicy}. {D.activeCount} active agents; {D.components.length} SCC{D.components.length===1?"":"s"}; {D.weakCount} connected group{D.weakCount===1?"":"s"}. Updates require a new snapshot.</Text>
 </Stack>;
}

function ActiveOnlyView(){
 const baseline=ACTIVE_ONLY.rows.find(r=>r.id==="NNUE_W512_v3");
 const [sort,setSort]=useCanvasState("active-only-sort","filtered");
 const rows=ACTIVE_ONLY.rows.slice().sort((a,b)=>sort==="change"?Math.abs(b.delta)-Math.abs(a.delta):sort==="full"?a.full_rank-b.full_rank:a.rank-b.rank);
 const stamp=new Date(ACTIVE_ONLY.asof).toLocaleString("en-US",{timeZone:"America/Los_Angeles",timeZoneName:"short"});
 return <Stack gap={16}>
 <H2>Ratings excluding retired opponents</H2>
 <Text>{ACTIVE_ONLY.included_games.toLocaleString()} retained games out of {ACTIVE_ONLY.total_games.toLocaleString()} · {ACTIVE_ONLY.excluded_games.toLocaleString()} excluded · {ACTIVE_ONLY.active_count} active agents · snapshot {stamp}</Text>
 {baseline&&<div className="rating-note"><strong>512 v3 moves from {baseline.full_rank}th to {baseline.rank}th.</strong> Its fitted rating changes from {baseline.full_rating.toFixed(1)} to {baseline.rating.toFixed(1)}.</div>}
 <Text>Exclude every completed game involving any of the {ACTIVE_ONLY.retired_count} retired agents in that saved roster, including games played before retirement. Refit the same Bradley–Terry model from scratch over the fixed current active field. Draws count as half a win each way; both fits center the same {ACTIVE_ONLY.active_count} active agents at 1500.</Text>
 <Text>Filtered fit: {ACTIVE_ONLY.fit_status}; {ACTIVE_ONLY.components.length} strongly connected component; {ACTIVE_ONLY.weak_components.length} connected group. Every displayed rating belongs to the same joint fit.</Text>
 <label className="rating-filter">Sort <select aria-label="Active-only rating sort" value={sort} onChange={e=>setSort(e.target.value)}><option value="filtered">Active-only rank</option><option value="full">Full-history rank</option><option value="change">Largest absolute rating change</option></select></label>
 <div className="rating-table-wrap"><table className="rating-table"><thead><tr><th>Active-only rank</th><th>Agent</th><th>Active-only rating</th><th>Full-history rating</th><th>Change</th><th>Full-history rank</th><th>Retained games</th><th>Retained W–D–L</th></tr></thead><tbody>{rows.map(r=><tr key={r.id} style={{background:r.id==="NNUE_W512_v3"?"var(--selected)":undefined}}><td>{r.rank}</td><td>{r.name}</td><td className="rating-number"><strong>{r.rating.toFixed(1)}</strong></td><td className="rating-number">{r.full_rating.toFixed(1)}</td><td className="rating-number">{r.delta>=0?"+":""}{r.delta.toFixed(1)}</td><td>{r.full_rank}</td><td>{r.games}</td><td className="rating-number">{r.w}–{r.d}–{r.l}</td></tr>)}</tbody></table></div>
 <Text>Change = active-only rating minus full-history rating. The filtered records describe retained games only. This sensitivity analysis uses existing results; it does not establish which fit better predicts future play.</Text>
 <p className="rating-caption">Source: current tournament state at {ACTIVE_ONLY.asof}. Retirement membership is fixed for this comparison. Live standings and automatic teacher selection continue to use the full-history fit.</p>
 <p className="rating-caption"><a href="nnue-active-only-comparison.json">Comparison data</a> · <a href="nnue-active-only-state.json">Filtered state</a> · <a href="nnue-active-only-fit.json">Filtered fit</a> · <a href="nnue-active-only-full-fit.json">Full-history fit</a> · <a href="nnue-active-only-source-state.json">Source state</a></p>
 </Stack>;
}

function GenerationsView(){
 const [selected,setSelected]=useCanvasState("generation","search");
 const groups=D.generationGroups;
 const pending=D.generationStatus!=="active";
 const error=D.generationDeployment?.stage==="failed"?D.generationDeployment.error:null;
 const aliases:Record<string,string>={v2:"v2_v3",v3:"v2_v3","v4_v4.5":"v4_v5",v5:"v4_v5",search_wave_1:"search",search_wave_2:"search"};
 const group=groups.find(g=>g.id===(aliases[selected]||selected))||groups[0];
 return <Stack gap={16}>
 <H2>One protected representative per generation</H2>
 <Text>Apply the normal retirement rule, then keep the highest-rated registered agent in each generation. Previously retired leaders can return. Other eligible agents continue playing normally. Representatives change automatically as ratings change.</Text>
 <div className="rating-note">{error?<><strong>Upgrade needs attention.</strong> {error} </>:pending?<><strong>Upgrade queued · waiting for the current games.</strong> The live roster still has {D.activeCount} active agents. The policy will reactivate the retired generation leaders below; no in-flight game will be interrupted. This page reloads every 30 seconds while activation is pending. </>:<strong>{groups.length} generations represented · {D.activeCount} active agents.</strong>} {groups.some(g=>g.retained)?`${groups.filter(g=>g.retained).map(g=>g.name).join(" and ")} ${pending?"will receive":"currently need"} generation protection.`:"Every generation leader is also eligible under the ordinary rule."} All historical games still count in the fitted ratings.</div>
 <div className="rating-table-wrap"><table className="rating-table"><thead><tr><th>Generation</th><th>Representative</th><th>Fitted rating</th><th>Active / registered</th><th>Eligibility</th></tr></thead><tbody>{groups.map(g=><tr key={g.id}><td>{g.label}</td><td><strong>{g.name}</strong></td><td className="rating-number">{g.rating.toFixed(1)}</td><td className="rating-number">{g.active} / {g.total}</td><td>{g.retained?pending?"Protected on activation":"Protected from retirement":"Eligible normally"}</td></tr>)}</tbody></table></div>
 <Text>{groups.length} generations from the current roster. Search transfers retain their assigned search generation; width does not define a generation. Exact rating ties use the agent ID alphabetically.</Text>
 <label className="rating-filter">Inspect generation <select aria-label="Inspect generation" value={group.id} onChange={e=>setSelected(e.target.value)}>{groups.map(g=><option key={g.id} value={g.id}>{g.label}</option>)}</select></label>
 <div className="rating-table-wrap"><table className="rating-table"><thead><tr><th>{group.label} member</th><th>Fitted rating</th><th>Status</th></tr></thead><tbody>{group.members.map(r=><tr key={r.id} style={{opacity:r.active?1:.65}}><td>{r.name}{r.representative?" · representative":""}</td><td className="rating-number">{r.rating.toFixed(1)}</td><td>{r.active?r.representative&&group.retained?"Active · generation protection":"Active":r.representative&&pending?"Retired · returns on activation":"Retired"}</td></tr>)}</tbody></table></div>
 <Text>Source: live tournament state and full-history Bradley–Terry fit at {D.snapshotLabel}. Ratings center the active field at 1500; restoring representatives can shift absolute displayed values without changing fitted gaps within a connected group. {pending?"Live ratings still use the original retirement policy until the upgrade completes.":"Snapshot view; refresh requires a new data download."} Target policy: {D.generationPolicy}. <a href="https://github.com/yetiowin805/taikyoku_shogi/pull/195">Implementation and validation</a>.</Text>
 </Stack>;
}

// Kept independent of the dashboard bundle: h is React's JSX element factory.
function ComparisonFlow({h, data, rows, theme: t, selected, onSelect}: any) {
  const nodes = data.components;
  const color = id => `var(--scc-${id}, hsl(${(id * 137.508) % 360} 55% 48%))`;
  const rank = new Map(nodes.map(n => [n.id, 0]));
  // Condensed SCC graphs are DAGs. Longest-path ranks keep all evidence forward.
  for (let pass = 0; pass < nodes.length; pass++) {
    let changed = false;
    for (const e of data.edges) {
      if (rank.get(e.b) <= rank.get(e.a)) {
        rank.set(e.b, rank.get(e.a) + 1);
        changed = true;
      }
    }
    if (!changed) break;
  }
  const layers = [];
  for (const n of nodes) (layers[rank.get(n.id)] ||= []).push(n);
  const count = Math.max(1, ...layers.map(layer => layer.length));
  const height = count * 170 + 170;
  const width = Math.max(740, (layers.length - 1) * 500 + 360);
  const positions = new Map();
  layers.forEach((layer, col) => layer.forEach((n, row) => positions.set(n.id, {
    x: 180 + col * 500,
    y: 50 + row * 170 + (count - layer.length) * 85,
  })));
  const routes = data.edges.map(e => {
    const a = positions.get(e.a), b = positions.get(e.b);
    const outgoing = data.edges.filter(v => v.a === e.a).sort((u,v) => positions.get(u.b).y - positions.get(v.b).y);
    const incoming = data.edges.filter(v => v.b === e.b).sort((u,v) => positions.get(u.a).y - positions.get(v.a).y);
    const x1 = a.x + 140, x2 = b.x - 140;
    const y1 = a.y + 104 * (outgoing.indexOf(e) + 1) / (outgoing.length + 1);
    const y2 = b.y + 104 * (incoming.indexOf(e) + 1) / (incoming.length + 1);
    const long = rank.get(e.b) - rank.get(e.a) > 1;
    // Skip-layer edges use a clear corridor beneath every card, never a card center.
    const lane = height - 38 - data.edges.filter(v => rank.get(v.b) - rank.get(v.a) > 1).indexOf(e) * 30;
    const mid = (x1 + x2) / 2;
    return {...e, x1, y1, x2, y2,
      d: long ? `M${x1},${y1} H${x1+45} V${lane} H${x2-45} V${y2} H${x2}` : `M${x1},${y1} C${mid},${y1} ${mid},${y2} ${x2},${y2}`,
      lx: mid, ly: long ? lane - 10 : (y1 + y2) / 2 - 10,
    };
  });
  const children = [];
  children.push(h('defs', {children: nodes.map(n => h('marker', {
    key: n.id, id: `flow-arrow-${n.id}`, viewBox: '0 0 10 10', refX: 10, refY: 5,
    markerWidth: 9, markerHeight: 9, markerUnits: 'userSpaceOnUse', orient: 'auto',
    children: h('path', {d: 'M0,0 L10,5 L0,10 z', fill: color(n.id)}),
  }))}));
  children.push(h('text', {x: 40, y: 24, fill: t.text.secondary, fontSize: 12,
    children: 'Completed scoring paths flow left → right · columns are connectivity, not rating tiers'}));
  for (const e of routes) {
    const related = selected === e.a || selected === e.b;
    const label = `SCC ${e.a+1} → SCC ${e.b+1} · ${e.points} points`;
    children.push(h('g', {key: `edge-${e.a}-${e.b}`, opacity: related ? 1 : 0.6, children: [
      h('title', {children: label}),
      h('path', {d: e.d, fill: 'none', stroke: t.bg.editor, strokeWidth: 8}),
      h('path', {d: e.d, fill: 'none', stroke: color(e.a), strokeWidth: related ? 3 : 2,
        markerEnd: `url(#flow-arrow-${e.a})`}),
      h('circle', {cx: e.x1, cy: e.y1, r: 4, fill: color(e.a)}),
      h('text', {x: e.lx, y: e.ly, textAnchor: 'middle', fill: color(e.a), fontSize: 11,
        fontWeight: related ? 700 : 500, stroke: t.bg.editor, strokeWidth: 5,
        paintOrder: 'stroke', strokeLinejoin: 'round', children: label}),
    ]}));
  }
  for (const n of nodes) {
    const p = positions.get(n.id), values = rows.filter(r => r.scc === n.id);
    const ratings = values.map(r => r.rating);
    const label = n.members.length === 1 ? n.members[0] : n.label === 'Established core' ? n.label : `${n.members[0] || 'Historical agents'} + ${Math.max(0,n.members.length-1)} others`;
    const rating = values.length ? values.every(r => !r.games) ? 'Unrated · 1500 placeholder' : `${Math.min(...ratings).toFixed(1)}–${Math.max(...ratings).toFixed(1)}` : 'Historical evidence only';
    children.push(h('g', {key: `node-${n.id}`, role: 'button', tabIndex: 0,
      'aria-label': `SCC ${n.id+1}: ${n.label}. Select to highlight direct connections.`,
      'aria-pressed': selected === n.id,
      onClick: () => onSelect(n.id), onKeyDown: e => {if(e.key === 'Enter' || e.key === ' ') {e.preventDefault(); onSelect(n.id);}},
      style: {cursor: 'pointer'}, children: [
        h('title', {children: `SCC ${n.id+1}: ${n.members.join(', ')}`}),
        h('rect', {x: p.x-140, y: p.y, width: 280, height: 104, rx: 5,
          fill: selected === n.id ? t.fill.secondary : t.bg.elevated, stroke: color(n.id), strokeWidth: selected === n.id ? 3 : 1}),
        h('text', {x: p.x-124, y: p.y+23, fill: color(n.id), fontSize: 15, fontWeight: 700, children: `SCC ${n.id+1}`}),
        h('text', {x: p.x-124, y: p.y+47, fill: t.text.primary, fontSize: 13, children: label.length > 34 ? label.slice(0,31)+'…' : label}),
        h('text', {x: p.x-124, y: p.y+69, fill: t.text.secondary, fontSize: 12, children: `${n.members.length} active${n.retired ? ` + ${n.retired} retired` : ''}`}),
        h('text', {x: p.x-124, y: p.y+90, fill: t.text.secondary, fontSize: 12, children: `${rating} · fitted rating`}),
      ]}));
  }
  const direct = data.edges.filter(e => e.a === selected || e.b === selected);
  return h('div', {children: [
    h('div', {style: {overflowX: 'auto'}, children: h('svg', {width, height, role: 'img',
      'aria-label': 'Directed SCC graph with explicit source and destination labels; arrows route around cards', children})}),
    h('p', {style: {margin: '6px 0', color: t.text.secondary}, children: `Selected SCC ${selected+1} · Direct completed links: ${direct.length ? direct.map(e => `SCC ${e.a+1} → SCC ${e.b+1} (${e.points} points)`).join('; ') : 'none'}. Select any box to highlight its links.`}),
    h('p', {style: {margin: '6px 0', color: t.accent.primary}, children: data.scheduler ? "Current assignments are listed in the Scheduler tab; only completed scores create graph edges." : !data.workerPair.agents.length ? "Waiting for enough new-rule results to select a supplemental pair." : data.workerEdge[0] !== data.workerEdge[1] ? `Current supplemental pair: SCC ${data.workerEdge[0]+1} ↔ SCC ${data.workerEdge[1]+1} · ${data.workerPair.agents.map(agentWithRating).join(' vs ')} · ${data.workerPair.statuses.join(' / ')}. This is not a completed scoring edge.` : `Current supplemental pair is inside SCC ${data.workerEdge[0]+1}.`}),
  ]});
}

function SchedulerView({h, data, theme}: any) {
  if (!data) return null;
  const text = children => h('p', {style:{margin:'6px 0',lineHeight:1.6},children});
  const cell = children => h('td', {style:{padding:'9px 12px',borderBottom:`1px solid ${theme.stroke.secondary}`},children});
  return h('section', {style:{padding:'12px 0'},children:[
    h('h2',{children:'Adaptive game scheduler'}),
    text(`${data.totalPairs} new pairs scheduled · ${data.completedPairs} finished · ${data.actualComponents} actual SCCs · ${data.assumedComponents} SCCs after treating unfinished commitments as draws for selection.`),
    text('All available game workers share one allocator. Connectivity comes first and reserves each pair before another worker selects. Once the assumed-draw graph is strongly connected, allocation targets 85% placement / top-five learning, 10% champion probes, and 5% generation anchors. Connectivity pairs do not consume those shares.'),
    text(`Allocated non-connectivity pairs: learning ${data.lanes[0]}, champion ${data.lanes[1]}, anchors ${data.lanes[2]}. Each pair reverses colors on the same start. First matchups use the normal opening; repeated matchups use light starts. Unfinished games never count as rating evidence.`),
    text('Ratings beside names use the latest full-history fit, rounded to whole points, including for inactive opponents. They are current ratings, not ratings when the game started. Unrated means no completed games; * marks provisional cross-SCC offsets.'),
    data.error ? text(`Scheduler error: ${data.error}`) : null,
    data.pairs.length ? h('div',{style:{overflowX:'auto'},children:h('table',{style:{width:'100%',borderCollapse:'collapse',fontVariantNumeric:'tabular-nums'},children:[
      h('thead',{children:h('tr',{children:['Agents','Reason','Slots','Completed points','Game status'].map(label=>h('th',{style:{textAlign:'left',padding:'9px 12px'},children:label}))})}),
      h('tbody',{children:data.pairs.map(pair=>h('tr',{key:pair.ids[0],children:[cell(h('div',{children:pair.agents.map((name,i)=>h('div',{key:name,style:{whiteSpace:'nowrap',lineHeight:1.7},children:[i?'vs ':'',name,h('span',{style:{color:theme.text.secondary,marginLeft:8,fontVariantNumeric:'tabular-nums'},children:agentRatingText(name)?` (${agentRatingText(name)})`:''})]}))})),cell(pair.reason),cell(pair.ids.join(' / ')),cell(pair.score.join('–')),cell(pair.statuses.join(' / '))]}))})
    ]})}) : null,
    text(`Source: live state.json adaptive ledger. Showing running legacy continuations, unfinished adaptive pairs, and the latest eight completed adaptive pairs. ${data.cancelled} unstarted assignments were archived at migration; old bracket results remain evidence. Legacy return legs can still finish outside this new-pair ledger.`)
  ]});
}

function agentRatingText(name) {
  const info = D.agentRatings && (D.agentRatings[name] || D.agentRatings[name.replace(/^512v3 /, '')]);
  if (!info) return '';
  if (!info.games) return 'unrated';
  return Math.round(info.rating).toLocaleString('en-US') + (info.provisional ? ` · SCC ${info.scc}*` : '');
}
function agentWithRating(name, lookupName = name) {
  const rating = agentRatingText(lookupName);
  return rating ? `${name} (${rating})` : name;
}

function GenerationStrengthView({h, theme}: any) {
 return h('iframe', {src:'generation-strength.html', title:'Generation strength estimates', style:{display:'block',width:'100%',height:'85vh',minHeight:760,border:`1px solid ${theme.stroke.secondary}`,borderRadius:4,background:theme.bg.editor}});
}
