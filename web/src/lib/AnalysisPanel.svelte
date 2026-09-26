<script>
  let {
    active = $bindable(false),
    models = [],
    model = $bindable('ab-seed.json'),
    maxDepth = $bindable(16),
    qDepth = $bindable(2),
    cpuPercent = $bindable(100),
    lineCount = $bindable(3),
    search = null,
    elapsedMs = 0,
    status = 'Ready',
    onStart = () => {},
    onStop = () => {},
    onRestart = () => {},
  } = $props();

  function blackAbsolute(score) {
    if (score == null) return null;
    return search?.side === 'White' ? -score : score;
  }

  function formatScore(score) {
    const value = blackAbsolute(score);
    if (value == null) return '—';
    if (value > 0) return `B+${value}`;
    if (value < 0) return `W+${Math.abs(value)}`;
    return '0';
  }

  function scoreClass(score) {
    const value = blackAbsolute(score);
    if (value == null || Math.abs(value) <= 50) return 'neutral';
    return value > 0 ? 'black-advantage' : 'white-advantage';
  }

  function principalVariation(tree) {
    const line = [];
    let node = tree;
    for (let i = 0; i < 8; i += 1) {
      const children = node?.children || [];
      if (!children.length) break;
      const next = children.find((child) => child.best) || children[0];
      line.push(next.label);
      node = next;
    }
    return line;
  }

  let candidates = $derived((search?.root_moves || []).slice(0, Number(lineCount) || 1));
  let pv = $derived(principalVariation(search?.tree));
  let nps = $derived(elapsedMs > 0 && search?.nodes ? Math.round(search.nodes * 1000 / elapsedMs) : 0);
</script>

<section class="panel analysis-panel">
  <div class="analysis-header">
    <div>
      <p class="eyebrow">ENGINE ANALYSIS</p>
      <h3>{active ? 'Calculating' : 'Analysis board'}</h3>
    </div>
    <button
      type="button"
      class:stop={active}
      class:start={!active}
      onclick={active ? onStop : onStart}
    >
      {active ? '■ Stop' : '▶ Start'}
    </button>
  </div>

  <div class="engine-row">
    <label for="analysis-model">Engine model</label>
    <select id="analysis-model" bind:value={model} onchange={onRestart}>
      {#each models as item}
        <option value={item}>{item}</option>
      {/each}
    </select>
  </div>

  <div class="telemetry" class:idle={!search}>
    <div class="evaluation">
      <span class="eval-label">Evaluation</span>
      <strong class={scoreClass(search?.score)}>{formatScore(search?.score)}</strong>
    </div>
    <div class="stats">
      <span><b>{search?.depth ?? 0}</b> depth</span>
      <span><b>{search?.nodes?.toLocaleString?.() ?? 0}</b> nodes</span>
      <span><b>{nps.toLocaleString()}</b> n/s</span>
    </div>
  </div>

  {#if search?.best_move}
    <div class="best-move">
      <span class="rank">1</span>
      <div>
        <span class="move">{search.best_move}</span>
        <span class="caption">best move at depth {search.depth}</span>
      </div>
      <strong class={scoreClass(search.score)}>{formatScore(search.score)}</strong>
    </div>
  {:else}
    <p class="empty-state">Start analysis to draw the best move and compare candidate lines.</p>
  {/if}

  {#if pv.length}
    <div class="pv-block">
      <span>Principal variation</span>
      <code>{pv.join('  ')}</code>
    </div>
  {/if}

  {#if candidates.length}
    <div class="candidate-list">
      {#each candidates as candidate, index}
        <div class:best={candidate.best} class="candidate">
          <span class="rank">{index + 1}</span>
          <span class="move">{candidate.label}</span>
          <strong class={scoreClass(candidate.score)}>{formatScore(candidate.score)}</strong>
        </div>
      {/each}
    </div>
  {/if}

  <details class="settings" open>
    <summary>Performance settings</summary>
    <div class="setting">
      <div><label for="analysis-cpu">CPU budget</label><output>{cpuPercent}%</output></div>
      <input id="analysis-cpu" type="range" min="10" max="100" step="10" bind:value={cpuPercent} onchange={onRestart} />
      <p>Throttles the running search without discarding its transposition table or completed depths.</p>
    </div>
    <div class="setting split">
      <label>Max depth <input type="number" min="1" max="64" bind:value={maxDepth} onchange={onRestart} /></label>
      <label>Q-depth <input type="number" min="0" max="8" bind:value={qDepth} onchange={onRestart} /></label>
      <label>Lines <input type="number" min="1" max="5" bind:value={lineCount} /></label>
    </div>
  </details>

  <div class="analysis-status"><span class:live={active}></span>{status}</div>
</section>

<style>
  .analysis-panel { padding: 0; overflow: hidden; border-color: #c7c1b2; }
  .analysis-header { display: flex; align-items: center; justify-content: space-between; padding: .85rem .9rem .65rem; background: #242621; color: #f4f0e6; }
  .analysis-header h3 { margin: .08rem 0 0; font-size: 1.05rem; }
  .eyebrow { margin: 0; color: #a9b2a4; font-size: .64rem; font-weight: 700; letter-spacing: .14em; }
  .analysis-header button { border: 0; border-radius: 999px; padding: .38rem .75rem; color: white; font-weight: 650; }
  .analysis-header button.start { background: #16845a; }
  .analysis-header button.stop { background: #a6493f; }
  .engine-row { display: grid; grid-template-columns: auto minmax(0, 1fr); align-items: center; gap: .65rem; padding: .65rem .8rem; border-bottom: 1px solid #d8d1c1; font-size: .78rem; }
  .engine-row label { color: #625c50; }
  .engine-row select { min-width: 0; width: 100%; }
  .telemetry { display: flex; align-items: stretch; border-bottom: 1px solid #d8d1c1; }
  .telemetry.idle { opacity: .55; }
  .evaluation { display: flex; flex-direction: column; justify-content: center; min-width: 105px; padding: .6rem .8rem; border-right: 1px solid #d8d1c1; }
  .eval-label { color: #746d60; font-size: .65rem; text-transform: uppercase; letter-spacing: .06em; }
  .evaluation strong { font: 700 1.35rem "IBM Plex Mono", ui-monospace, monospace; }
  .stats { display: grid; grid-template-columns: repeat(3, 1fr); flex: 1; align-items: center; gap: .25rem; padding: .55rem .65rem; color: #746d60; font-size: .62rem; text-align: center; }
  .stats b { display: block; color: #272822; font: 650 .75rem "IBM Plex Mono", ui-monospace, monospace; }
  .best-move, .candidate { display: grid; grid-template-columns: 1.4rem minmax(0, 1fr) auto; align-items: center; gap: .45rem; }
  .best-move { padding: .72rem .8rem; background: #eef6ef; border-bottom: 1px solid #d0dfd2; }
  .rank { display: grid; place-items: center; width: 1.25rem; height: 1.25rem; border-radius: 50%; background: #d9ded6; color: #4a5048; font-size: .65rem; font-weight: 700; }
  .best-move .rank { background: #16845a; color: white; }
  .move { overflow: hidden; font: 650 .77rem "IBM Plex Mono", ui-monospace, monospace; text-overflow: ellipsis; white-space: nowrap; }
  .caption { display: block; margin-top: .1rem; color: #6d756b; font-size: .64rem; }
  .pv-block { padding: .55rem .8rem; border-bottom: 1px solid #d8d1c1; }
  .pv-block > span { display: block; margin-bottom: .25rem; color: #746d60; font-size: .65rem; text-transform: uppercase; letter-spacing: .05em; }
  .pv-block code { display: block; overflow-x: auto; color: #383a34; font-size: .69rem; white-space: nowrap; }
  .candidate-list { border-bottom: 1px solid #d8d1c1; }
  .candidate { padding: .42rem .8rem; border-top: 1px solid #ebe6da; }
  .candidate.best { background: #f3f8f3; }
  .candidate strong, .best-move > strong { font: 650 .72rem "IBM Plex Mono", ui-monospace, monospace; }
  .empty-state { margin: 0; padding: 1rem .85rem; color: #696256; font-size: .76rem; line-height: 1.45; border-bottom: 1px solid #d8d1c1; }
  .settings { padding: .55rem .8rem .7rem; border-bottom: 1px solid #d8d1c1; }
  .settings summary { cursor: pointer; color: #4f4a40; font-size: .72rem; font-weight: 700; }
  .setting { margin-top: .65rem; }
  .setting > div { display: flex; justify-content: space-between; align-items: baseline; font-size: .72rem; }
  .setting output { color: #16845a; font: 650 .7rem "IBM Plex Mono", ui-monospace, monospace; }
  .setting input[type="range"] { width: 100%; accent-color: #16845a; }
  .setting p { margin: .1rem 0 0; color: #7a7367; font-size: .63rem; line-height: 1.3; }
  .setting.split { display: grid; grid-template-columns: repeat(2, 1fr); gap: .45rem .65rem; }
  .setting.split label { display: flex; align-items: center; justify-content: space-between; gap: .4rem; color: #5f594e; font-size: .68rem; }
  .setting.split input { width: 3.4rem; }
  .analysis-status { padding: .5rem .8rem; color: #6e675b; font-size: .67rem; }
  .analysis-status span { display: inline-block; width: .45rem; height: .45rem; margin-right: .4rem; border-radius: 50%; background: #aaa396; }
  .analysis-status span.live { background: #18a36b; box-shadow: 0 0 0 3px rgba(24, 163, 107, .13); }
  .black-advantage { color: #176b47; }
  .white-advantage { color: #9b3c35; }
  .neutral { color: #4b4b46; }
</style>
