<script>
  import Board from './lib/Board.svelte';
  import SearchPanel from './lib/SearchPanel.svelte';
  import * as api from './lib/api.js';

  let mode = $state('play'); // play | debug
  let snapshot = $state(null);
  let games = $state([]);
  let selectedGame = $state('');
  let logLines = $state([]);
  let selected = $state(null);
  let highlights = $state([]);
  let pendingMoves = $state([]);
  let blackController = $state('human');
  let whiteController = $state('mi');
  let autoPlay = $state(false);
  let busy = $state(false);
  let gotoPly = $state(0);
  let models = $state(['ab-seed.json']);
  let blackAbModel = $state('ab-seed.json');
  let whiteAbModel = $state('ab-seed-noisy.json');
  let abDepth = $state(2);
  let abQDepth = $state(2);
  let abTimeMs = $state(0); // 0 = unlimited
  let runActive = $state(false);
  let runLabel = $state('');
  let blackSearch = $state(null);
  let whiteSearch = $state(null);
  let blackSearchExpanded = $state(false);
  let whiteSearchExpanded = $state(false);

  function abOpts(modelFile) {
    const opts = {
      depth: Number(abDepth) || 2,
      quiescence_depth: Number(abQDepth) || 0,
      model: `models/${modelFile}`,
    };
    const t = Number(abTimeMs);
    if (t > 0) opts.max_time_ms = t;
    return opts;
  }

  function modelForSide(side) {
    return side === 'White' ? whiteAbModel : blackAbModel;
  }

  function agentOptsFor(name, side) {
    if (name !== 'ab' && name !== 'search') return {};
    return abOpts(modelForSide(side || snapshot?.turn || 'Black'));
  }

  function log(msg, kind = 'ok') {
    const t = new Date().toLocaleTimeString();
    logLines = [`[${t}] ${msg}`, ...logLines].slice(0, 200);
    // kind unused in storage but we prefix errors
    if (kind === 'err') {
      logLines[0] = `[${t}] ERROR: ${msg}`;
    }
  }

  function applyResult(res, silent = false) {
    if (!res) return;
    snapshot = res.snapshot;
    gotoPly = res.snapshot?.cursor ?? 0;
    if (res.search) {
      const side = res.search.side || snapshot?.turn;
      if (side === 'White') whiteSearch = res.search;
      else blackSearch = res.search;
    }
    if (!silent) {
      if (res.ok) log(res.message, 'ok');
      else log(res.message, 'err');
    } else if (!res.ok) {
      log(res.message, 'err');
    }
    if (res.moves) pendingMoves = res.moves;
  }

  async function refresh() {
    try {
      const res = await api.getState();
      applyResult(res, true);
    } catch (e) {
      log(String(e), 'err');
    }
  }

  async function refreshGames() {
    try {
      const res = await api.listGames();
      if (res.ok) {
        games = res.games || [];
        if (!selectedGame && games.length) selectedGame = games[0];
      } else {
        log(res.message || 'list failed', 'err');
      }
    } catch (e) {
      log(String(e), 'err');
    }
  }

  async function refreshModels() {
    try {
      const res = await api.listModels();
      if (res.ok && res.models?.length) {
        models = res.models;
        if (!models.includes(blackAbModel)) blackAbModel = models[0];
        if (!models.includes(whiteAbModel)) {
          whiteAbModel =
            models.find((m) => m !== blackAbModel) || models[0];
        }
      }
    } catch (e) {
      log(String(e), 'err');
    }
  }

  async function onNew() {
    const res = await api.newGame();
    selected = null;
    highlights = [];
    pendingMoves = [];
    applyResult(res);
  }

  async function onLoad() {
    if (!selectedGame) return;
    const res = await api.loadGame(selectedGame);
    selected = null;
    highlights = [];
    pendingMoves = [];
    applyResult(res);
  }

  async function onSave() {
    const res = await api.saveGame(null);
    applyResult(res);
  }

  async function onSuggest() {
    const side = snapshot?.turn || 'Black';
    const agent = side === 'White' ? whiteController : blackController;
    const name = agent === 'human' ? 'mi' : agent;
    const res = await api.suggest(name, agentOptsFor(name, side));
    applyResult(res);
  }

  async function runAgentIfNeeded() {
    if (mode !== 'play' || !autoPlay || !snapshot || busy) return;
    if (snapshot.winner || snapshot.draw) {
      if (runActive) {
        log(`Run finished: ${snapshot.winner ? `winner ${snapshot.winner}` : snapshot.draw}`);
        runActive = false;
        autoPlay = false;
      }
      return;
    }
    const turn = snapshot.turn;
    const ctrl = turn === 'Black' ? blackController : whiteController;
    if (ctrl === 'human') return;
    busy = true;
    try {
      const res = await api.playAgent(ctrl, agentOptsFor(ctrl, turn));
      applyResult(res);
      selected = null;
      highlights = [];
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    // kick autoplay when state/controllers change
    snapshot;
    blackController;
    whiteController;
    autoPlay;
    mode;
    if (mode === 'play' && autoPlay) {
      const id = setTimeout(() => runAgentIfNeeded(), 150);
      return () => clearTimeout(id);
    }
  });

  async function onCellClick({ file, rank }) {
    if (!snapshot) return;
    if (mode === 'play') {
      const ctrl = snapshot.turn === 'Black' ? blackController : whiteController;
      if (ctrl !== 'human') {
        log(`Side to move is controlled by ${ctrl}`, 'err');
        return;
      }
    }

    const piece = (snapshot.pieces || []).find(
      (p) => p.file === file && p.rank === rank,
    );

    if (!selected) {
      if (!piece || piece.color !== snapshot.turn) {
        log('Select one of your pieces', 'err');
        return;
      }
      selected = { file, rank };
      const res = await api.getMoves(file, rank);
      applyResult(res, true);
      if (!res.ok) {
        log(res.message, 'err');
        selected = null;
        return;
      }
      const occ = new Set(
        (snapshot.pieces || []).map((p) => `${p.file},${p.rank}`),
      );
      highlights = (res.moves || []).map((m) => ({
        file: m.to_file,
        rank: m.to_rank,
        capture: occ.has(`${m.to_file},${m.to_rank}`),
      }));
      pendingMoves = res.moves || [];
      log(`Selected ${piece.symbol} at ${file},${rank} — ${pendingMoves.length} moves`);
      return;
    }

    // second click: try move or reselect
    if (selected.file === file && selected.rank === rank) {
      selected = null;
      highlights = [];
      pendingMoves = [];
      return;
    }

    if (piece && piece.color === snapshot.turn) {
      selected = { file, rank };
      const res = await api.getMoves(file, rank);
      applyResult(res, true);
      const occ = new Set(
        (snapshot.pieces || []).map((p) => `${p.file},${p.rank}`),
      );
      highlights = (res.moves || []).map((m) => ({
        file: m.to_file,
        rank: m.to_rank,
        capture: occ.has(`${m.to_file},${m.to_rank}`),
      }));
      pendingMoves = res.moves || [];
      return;
    }

    const matches = pendingMoves.filter(
      (m) => m.to_file === file && m.to_rank === rank,
    );
    if (!matches.length) {
      log('Not a legal destination', 'err');
      return;
    }
    let pathIndex = null;
    let promote = null;
    if (matches.length > 1) {
      // path_index is into the matching from→to list (0-based)
      pathIndex = 0;
      log(`Ambiguous (${matches.length} paths); using path_index 0`, 'ok');
    } else if (matches[0].promoted) {
      promote = true;
    }

    const body = {
      from_file: selected.file,
      from_rank: selected.rank,
      to_file: file,
      to_rank: rank,
      promote,
      path_index: pathIndex,
    };
    const res = await api.applyMove(body);
    applyResult(res);
    selected = null;
    highlights = [];
    pendingMoves = [];
  }

  async function stepBack() {
    const res = await api.back(1);
    selected = null;
    highlights = [];
    applyResult(res);
  }

  async function stepForward() {
    const res = await api.forward(1);
    selected = null;
    highlights = [];
    applyResult(res);
  }

  async function doGoto() {
    const res = await api.gotoPly(Number(gotoPly) || 0);
    selected = null;
    highlights = [];
    applyResult(res);
  }

  async function playOnce() {
    const turn = snapshot?.turn || 'Black';
    const ctrl = turn === 'Black' ? blackController : whiteController;
    const agent = ctrl === 'human' ? 'mi' : ctrl;
    const res = await api.playAgent(agent, agentOptsFor(agent, turn));
    selected = null;
    highlights = [];
    applyResult(res);
  }

  async function startRun(black, white, label, modelsPair) {
    mode = 'play';
    blackController = black;
    whiteController = white;
    if (modelsPair) {
      blackAbModel = modelsPair[0];
      whiteAbModel = modelsPair[1];
    }
    runLabel = label;
    runActive = true;
    blackSearch = null;
    whiteSearch = null;
    blackSearchExpanded = false;
    whiteSearchExpanded = false;
    selected = null;
    highlights = [];
    pendingMoves = [];
    const res = await api.newGame();
    applyResult(res);
    autoPlay = true;
    log(
      `Started run: ${label} (ab depth=${abDepth}, q=${abQDepth}, Black=${blackAbModel}, White=${whiteAbModel}${abTimeMs > 0 ? `, time=${abTimeMs}ms` : ''})`,
    );
  }

  async function stopRun(save = false) {
    autoPlay = false;
    runActive = false;
    log(`Stopped run${runLabel ? `: ${runLabel}` : ''}`);
    runLabel = '';
    if (save) {
      const res = await api.saveGame(null);
      applyResult(res);
    }
  }

  $effect(() => {
    refresh();
    refreshGames();
    refreshModels();
  });
</script>

<div class="app">
  <div class="toolbar">
    <strong>Taikyoku</strong>
    <button
      class="mode-btn"
      class:active={mode === 'play'}
      onclick={() => (mode = 'play')}>Play</button
    >
    <button
      class="mode-btn"
      class:active={mode === 'debug'}
      onclick={() => (mode = 'debug')}>Debug</button
    >
    <button onclick={onNew}>New game</button>
    <select bind:value={selectedGame}>
      {#each games as g}
        <option value={g}>{g}</option>
      {/each}
    </select>
    <button onclick={onLoad}>Load</button>
    <button onclick={onSave}>Save</button>
    <button onclick={refreshGames}>Refresh list</button>
    <span class="spacer"></span>
    {#if snapshot}
      <span
        >Turn: <strong>{snapshot.turn}</strong> · ply {snapshot.cursor}/{snapshot.timeline_len} · legal {snapshot.legal_move_count}</span
      >
    {/if}
  </div>

  <div class="main">
    <div class="board-wrap">
      <Board
        pieces={snapshot?.pieces || []}
        {selected}
        {highlights}
        {onCellClick}
      />
    </div>

    <div class="side">
      {#if mode === 'play'}
        <div class="panel">
          <h3>Controllers</h3>
          <div class="row">
            <label>Black</label>
            <select bind:value={blackController}>
              <option value="human">Human</option>
              <option value="mi">mi</option>
              <option value="random">random</option>
              <option value="royal">royal</option>
              <option value="ab">ab</option>
            </select>
          </div>
          <div class="row">
            <label>White</label>
            <select bind:value={whiteController}>
              <option value="human">Human</option>
              <option value="mi">mi</option>
              <option value="random">random</option>
              <option value="royal">royal</option>
              <option value="ab">ab</option>
            </select>
          </div>
          <div class="row">
            <label
              ><input type="checkbox" bind:checked={autoPlay} /> Auto-play AI
              turns</label
            >
          </div>
          <div class="row">
            <button onclick={playOnce} disabled={busy}>Play one AI move</button>
            <button onclick={onSuggest} disabled={busy}>Suggest</button>
          </div>
        </div>

        <div class="panel">
          <h3>Alpha-beta (ab)</h3>
          <div class="row">
            <label>Black model</label>
            <select bind:value={blackAbModel}>
              {#each models as m}
                <option value={m}>{m}</option>
              {/each}
            </select>
          </div>
          <div class="row">
            <label>White model</label>
            <select bind:value={whiteAbModel}>
              {#each models as m}
                <option value={m}>{m}</option>
              {/each}
            </select>
            <button onclick={refreshModels} title="Refresh models">↻</button>
          </div>
          <div class="row">
            <label>Depth</label>
            <input
              type="number"
              min="1"
              max="4"
              bind:value={abDepth}
              style="width:4rem"
            />
          </div>
          <div class="row">
            <label>Q-depth</label>
            <input
              type="number"
              min="0"
              max="8"
              bind:value={abQDepth}
              style="width:4rem"
              title="Capture-only quiescence depth (0 = off)"
            />
            <span class="hint">0=off</span>
          </div>
          <div class="row">
            <label>Time ms</label>
            <input
              type="number"
              min="0"
              step="100"
              bind:value={abTimeMs}
              style="width:5rem"
              title="0 = no time limit"
            />
            <span class="hint">0=off</span>
          </div>
        </div>

        <div class="panel">
          <h3>Runs</h3>
          {#if runActive}
            <p class="hint">Active: {runLabel || 'custom'} {busy ? '· thinking…' : ''}</p>
          {/if}
          <div class="row wrap">
            <button
              onclick={() =>
                startRun('ab', 'ab', 'seed vs noisy', [
                  'ab-seed.json',
                  'ab-seed-noisy.json',
                ])}
              disabled={busy}
              title="Black=ab-seed, White=ab-seed-noisy">seed vs noisy</button
            >
            <button
              onclick={() => startRun('ab', 'ab', 'ab vs ab')}
              disabled={busy}>ab vs ab</button
            >
            <button
              onclick={() => startRun('ab', 'mi', 'ab vs mi')}
              disabled={busy}>ab vs mi</button
            >
            <button
              onclick={() => startRun('mi', 'ab', 'mi vs ab')}
              disabled={busy}>mi vs ab</button
            >
          </div>
          <div class="row wrap">
            <button
              onclick={() => startRun(blackController === 'human' ? 'ab' : blackController, whiteController === 'human' ? 'ab' : whiteController, `${blackController} vs ${whiteController}`)}
              disabled={busy}>Start with controllers</button
            >
            <button onclick={() => stopRun(false)} disabled={!autoPlay && !runActive}
              >Stop</button
            >
            <button onclick={() => stopRun(true)} disabled={!autoPlay && !runActive}
              >Stop + save</button
            >
          </div>
        </div>
      {:else}
        <div class="panel">
          <h3>Scrubber</h3>
          <div class="row">
            <button onclick={stepBack}>◀ Back</button>
            <button onclick={stepForward}>Forward ▶</button>
          </div>
          <div class="row">
            <input
              type="range"
              min="0"
              max={snapshot?.timeline_len || 0}
              bind:value={gotoPly}
              onchange={doGoto}
            />
          </div>
          <div class="row">
            <input type="number" min="0" bind:value={gotoPly} style="width:5rem" />
            <button onclick={doGoto}>Goto ply</button>
          </div>
          <div class="row">
            <button onclick={onSuggest}>Suggest mi</button>
            <button onclick={playOnce}>Play mi here</button>
          </div>
        </div>
      {/if}

      <div class="panel status">
        <h3>Status</h3>
        <pre>{snapshot?.status_text || 'Loading…'}</pre>
        {#if snapshot?.winner}
          <p><strong>Winner: {snapshot.winner}</strong></p>
        {/if}
        {#if snapshot?.draw}
          <p><strong>Draw: {snapshot.draw}</strong></p>
        {/if}
        {#if snapshot}
          <p>
            Check — Black: {snapshot.black_in_check ? 'yes' : 'no'}, White:
            {snapshot.white_in_check ? 'yes' : 'no'}
          </p>
        {/if}
      </div>

      <SearchPanel
        title="Black search"
        search={blackSearch}
        bind:expanded={blackSearchExpanded}
      />
      <SearchPanel
        title="White search"
        search={whiteSearch}
        bind:expanded={whiteSearchExpanded}
      />
    </div>
  </div>

  <div class="log">
    {#each logLines as line}
      <div class={line.includes('ERROR') ? 'err' : 'ok'}>{line}</div>
    {:else}
      <div>Log: moves, agents, errors…</div>
    {/each}
  </div>
</div>
