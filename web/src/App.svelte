<script>
  import Board from './lib/Board.svelte';
  import AnalysisPanel from './lib/AnalysisPanel.svelte';
  import EvalSparkline from './lib/EvalSparkline.svelte';
  import SearchPanel from './lib/SearchPanel.svelte';
  import * as api from './lib/api.js';

  let mode = $state('play'); // play | analysis | debug
  let snapshot = $state(null);
  let games = $state([]);
  let selectedGame = $state('');
  let gameFilter = $state('');
  let logLines = $state([]);
  let selected = $state(null);
  let highlights = $state([]);
  let pendingMoves = $state([]);
  let leg = $state(null);
  let promoPrompt = $state(null);

  let boardPieces = $derived.by(() => {
    const pieces = snapshot?.pieces || [];
    if (!leg || !selected) return pieces;
    return pieces
      .filter(
        (p) =>
          !(
            p.file === leg.file &&
            p.rank === leg.rank &&
            p.color !== snapshot.turn
          ),
      )
      .map((p) =>
        p.file === selected.file && p.rank === selected.rank
          ? { ...p, file: leg.file, rank: leg.rank }
          : p,
      );
  });
  let boardSelected = $derived(leg || selected);
  let blackController = $state('human');
  let whiteController = $state('mi');
  let autoPlay = $state(false);
  let busy = $state(false);
  let gotoPly = $state(0);
  let evalSeries = $state(null);
  let models = $state(['ab-seed.json']);
  let blackAbModel = $state('ab-seed.json');
  let whiteAbModel = $state('ab-seed.json');
  let abDepth = $state(8);
  let abQDepth = $state(2);
  let abTimeMs = $state(3000); // tournament-style three-second searches
  let runActive = $state(false);
  let runLabel = $state('');
  let blackSearch = $state(null);
  let whiteSearch = $state(null);
  let blackSearchExpanded = $state(false);
  let whiteSearchExpanded = $state(false);
  let analysisActive = $state(false);
  let analysisSearch = $state(null);
  let analysisModel = $state('ab-seed.json');
  let analysisMaxDepth = $state(16);
  let analysisQDepth = $state(2);
  let analysisCpuPercent = $state(100);
  let analysisLineCount = $state(3);
  let analysisElapsedMs = $state(0);
  let analysisStatus = $state('Ready');
  let analysisJobId = $state(null);
  let analysisEpoch = 0;

  let analysisArrows = $derived.by(() =>
    (analysisSearch?.root_moves || [])
      .slice(0, Math.max(1, Number(analysisLineCount) || 1))
      .filter((candidate) => candidate.mv)
      .map((candidate) => ({ ...candidate.mv, best: candidate.best })),
  );

  let navEpoch = 0;

  function delay(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  function beginNav() {
    navEpoch += 1;
    return navEpoch;
  }

  function navIsCurrent(epoch) {
    return epoch === navEpoch;
  }

  function clearGesture() {
    selected = null;
    highlights = [];
    pendingMoves = [];
    leg = null;
    promoPrompt = null;
  }

  function hasVia(move) {
    return move?.via_file != null && move?.via_rank != null;
  }

  function uniqueMarks(entries) {
    const map = new Map();
    for (const entry of entries) {
      const key = `${entry.file},${entry.rank}`;
      const prev = map.get(key);
      if (!prev) map.set(key, { ...entry });
      else {
        prev.capture = prev.capture || entry.capture;
        prev.stop = prev.stop || entry.stop;
      }
    }
    return [...map.values()];
  }

  function firstMarks(moves) {
    const occupied = new Set(
      (snapshot?.pieces || []).map((p) => `${p.file},${p.rank}`),
    );
    return uniqueMarks(
      moves.flatMap((move) => {
        if (hasVia(move)) {
          return [
            {
              file: move.via_file,
              rank: move.via_rank,
              capture: occupied.has(`${move.via_file},${move.via_rank}`),
              stop: false,
            },
          ];
        }
        return [
          {
            file: move.to_file,
            rank: move.to_rank,
            capture: occupied.has(`${move.to_file},${move.to_rank}`),
            stop: false,
          },
        ];
      }),
    );
  }

  function secondMarks(moves, via) {
    const occupied = new Set(
      (snapshot?.pieces || []).map((p) => `${p.file},${p.rank}`),
    );
    const marks = moves
      .filter((m) => hasVia(m) && m.via_file === via.file && m.via_rank === via.rank)
      .map((move) => ({
        file: move.to_file,
        rank: move.to_rank,
        capture: occupied.has(`${move.to_file},${move.to_rank}`),
        stop: false,
      }));
    const canStop = moves.some(
      (m) => !hasVia(m) && m.to_file === via.file && m.to_rank === via.rank,
    );
    if (canStop) {
      marks.push({ file: via.file, rank: via.rank, capture: false, stop: true });
    }
    return uniqueMarks(marks);
  }

  function declineSquare(file, rank, avoid) {
    const blocked = new Set(
      [...avoid, ...boardPieces].map((s) => `${s.file},${s.rank}`),
    );
    // Prefer adjacent cells, then the nearest empty cell on a crowded board.
    for (let distance = 1; distance <= 70; distance++) {
      for (let dx = -distance; dx <= distance; dx++) {
        const dy = distance - Math.abs(dx);
        for (const y of new Set([rank - dy, rank + dy])) {
          const x = file + dx;
          if (x >= 1 && x <= 36 && y >= 1 && y <= 36 && !blocked.has(`${x},${y}`)) {
            return { file: x, rank: y };
          }
        }
      }
    }
    return null;
  }

  function sameAnalysis(prev, next) {
    if (!prev || !next) return false;
    if (
      prev.depth !== next.depth ||
      prev.nodes !== next.nodes ||
      prev.score !== next.score ||
      prev.best_move !== next.best_move ||
      prev.side !== next.side
    ) {
      return false;
    }
    const left = prev.root_moves || [];
    const right = next.root_moves || [];
    if (left.length !== right.length) return false;
    for (let i = 0; i < left.length; i++) {
      if (
        left[i].label !== right[i].label ||
        left[i].score !== right[i].score ||
        left[i].best !== right[i].best
      ) {
        return false;
      }
      const a = left[i].mv;
      const b = right[i].mv;
      if (!a || !b) {
        if (a !== b) return false;
        continue;
      }
      if (
        a.from_file !== b.from_file ||
        a.from_rank !== b.from_rank ||
        a.to_file !== b.to_file ||
        a.to_rank !== b.to_rank ||
        a.promoted !== b.promoted ||
        a.via_file !== b.via_file ||
        a.via_rank !== b.via_rank
      ) {
        return false;
      }
    }
    return true;
  }

  function stopAnalysis(message = 'Analysis paused') {
    const jobId = analysisJobId;
    analysisActive = false;
    analysisJobId = null;
    analysisEpoch += 1;
    analysisStatus = message;
    if (jobId != null) void api.stopAnalysis(jobId).catch(() => {});
  }

  async function runAnalysis(epoch) {
    try {
      const started = await api.startAnalysis({
        depth: Math.max(1, Number(analysisMaxDepth) || 16),
        quiescence_depth: Math.max(0, Number(analysisQDepth) || 0),
        model: `models/${analysisModel}`,
        cpu_percent: Math.min(100, Math.max(10, Number(analysisCpuPercent) || 100)),
      });
      if (!started?.ok || started.job_id == null) {
        throw new Error(started?.message || 'Could not start analysis');
      }
      if (!analysisActive || epoch !== analysisEpoch || mode !== 'analysis') {
        void api.stopAnalysis(started.job_id).catch(() => {});
        return;
      }
      analysisJobId = started.job_id;
      while (analysisActive && epoch === analysisEpoch && mode === 'analysis') {
        const view = await api.getAnalysis(started.job_id);
        if (!analysisActive || epoch !== analysisEpoch || mode !== 'analysis') return;
        if (!view?.ok) throw new Error(view?.message || 'Analysis update failed');
        if (view.search && !sameAnalysis(analysisSearch, view.search)) {
          analysisSearch = view.search;
        }
        analysisElapsedMs = Number(view.elapsed_ms) || 0;
        analysisStatus = view.message || 'Analyzing…';
        if (!view.running) {
          analysisActive = false;
          analysisJobId = null;
          return;
        }
        await delay(200);
      }
    } catch (error) {
      if (epoch !== analysisEpoch) return;
      analysisActive = false;
      analysisJobId = null;
      analysisStatus = `Analysis failed: ${String(error)}`;
      log(String(error), 'err');
    }
  }

  function startAnalysis() {
    if (!snapshot) return;
    autoPlay = false;
    mode = 'analysis';
    analysisActive = true;
    analysisEpoch += 1;
    const epoch = analysisEpoch;
    analysisStatus = 'Starting engine…';
    void runAnalysis(epoch);
  }

  function restartAnalysis() {
    if (!analysisActive || mode !== 'analysis') return;
    const oldJobId = analysisJobId;
    analysisEpoch += 1;
    analysisJobId = null;
    if (oldJobId != null) void api.stopAnalysis(oldJobId).catch(() => {});
    const epoch = analysisEpoch;
    analysisStatus = 'Applying settings…';
    void runAnalysis(epoch);
  }

  function positionChanged() {
    analysisSearch = null;
    analysisElapsedMs = 0;
    if (analysisActive && mode === 'analysis') restartAnalysis();
  }

  function setMode(next) {
    if (next !== 'analysis' && analysisActive) stopAnalysis();
    mode = next;
  }

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

  function applyResult(res, silent = false, { clearSearch = false } = {}) {
    if (!res) return;
    snapshot = res.snapshot;
    gotoPly = res.snapshot?.cursor ?? 0;
    // Always take the field when present (including null) so New/Load clear the chart.
    if ('eval_series' in res) {
      evalSeries = res.eval_series || null;
    }
    if (clearSearch) {
      blackSearch = null;
      whiteSearch = null;
    }
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

  /** Black-absolute score → B+/W+ label. */
  function formatBlackAbs(v) {
    if (v == null) return '—';
    if (v > 0) return `B+${v}`;
    if (v < 0) return `W+${Math.abs(v)}`;
    return '0';
  }

  function scoreClass(s) {
    if (s == null) return '';
    if (s > 50) return 'pos';
    if (s < -50) return 'neg';
    return 'neu';
  }

  function hasRecordedEval(rec) {
    return rec && (rec.eval != null || rec.static_eval != null);
  }

  let recordedHasData = $derived.by(() => {
    const r = snapshot?.recorded;
    return hasRecordedEval(r?.last) || hasRecordedEval(r?.next);
  });

  async function refresh() {
    try {
      const res = await api.getState();
      applyResult(res, true);
    } catch (e) {
      log(String(e), 'err');
    }
  }

  function gameLabel(path) {
    const parts = String(path).replace(/\\/g, '/').split('/');
    return parts[parts.length - 1] || path;
  }

  function gameGroup(path) {
    const parts = String(path).replace(/\\/g, '/').split('/');
    if (parts.length <= 1) return '(root)';
    return parts.slice(0, -1).join('/');
  }

  let filteredGames = $derived.by(() => {
    const q = gameFilter.trim().toLowerCase();
    if (!q) return games;
    return games.filter((g) => g.toLowerCase().includes(q));
  });

  let gameGroups = $derived.by(() => {
    /** @type {Map<string, string[]>} */
    const map = new Map();
    for (const g of filteredGames) {
      const key = gameGroup(g);
      if (!map.has(key)) map.set(key, []);
      map.get(key).push(g);
    }
    return [...map.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  });

  async function refreshGames() {
    try {
      const res = await api.listGames();
      if (res.ok) {
        games = res.games || [];
        if (selectedGame && !games.includes(selectedGame)) {
          selectedGame = '';
        }
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
        if (!models.includes(analysisModel)) analysisModel = models[0];
      }
    } catch (e) {
      log(String(e), 'err');
    }
  }

  async function onNew() {
    const epoch = beginNav();
    const res = await api.newGame();
    if (!navIsCurrent(epoch)) return;
    clearGesture();
    applyResult(res);
    if (res.ok) positionChanged();
  }

  async function onLoad() {
    if (!selectedGame) return;
    const epoch = beginNav();
    const res = await api.loadGame(selectedGame);
    if (!navIsCurrent(epoch)) return;
    clearGesture();
    applyResult(res, false, { clearSearch: true });
    if (res.ok) positionChanged();
    if (res.ok && res.eval_series) {
      log(
        `Eval chart: ${res.eval_series.source} (${res.eval_series.points?.length || 0} pts)`,
        'ok',
      );
    } else if (res.ok) {
      log('No eval chart for this game (no recorded scores / eval-trace)', 'ok');
    }
  }

  async function onSave() {
    const res = await api.saveGame(null);
    applyResult(res);
  }

  async function onSuggest() {
    const epoch = beginNav();
    const side = snapshot?.turn || 'Black';
    const agent = side === 'White' ? whiteController : blackController;
    const name = agent === 'human' ? 'mi' : agent;
    const res = await api.suggest(name, agentOptsFor(name, side));
    if (!navIsCurrent(epoch)) return;
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
    const epoch = beginNav();
    busy = true;
    try {
      const res = await api.playAgent(ctrl, agentOptsFor(ctrl, turn));
      if (!navIsCurrent(epoch)) return;
      applyResult(res);
      clearGesture();
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

  function movingPiece() {
    return (snapshot?.pieces || []).find(
      (p) => p.file === selected?.file && p.rank === selected?.rank,
    );
  }

  async function sendMove(matches, promote, via) {
    if (!selected || !matches?.length) return;
    const dest = matches[0];
    const epoch = beginNav();
    const res = await api.applyMove({
      from_file: selected.file,
      from_rank: selected.rank,
      to_file: dest.to_file,
      to_rank: dest.to_rank,
      promote,
      path_index: matches.length > 1 && !via ? 0 : null,
      via_file: via?.file ?? null,
      via_rank: via?.rank ?? null,
      direct: !via,
    });
    if (!navIsCurrent(epoch)) return;
    applyResult(res);
    if (res.ok) positionChanged();
    clearGesture();
  }

  function offerPromotion(matches, via) {
    const promoted = matches.some((m) => m.promoted);
    const plain = matches.some((m) => !m.promoted);
    if (!(promoted && plain)) {
      void sendMove(matches, promoted ? true : null, via);
      return;
    }
    const dest = matches[0];
    const piece = movingPiece();
    const avoid = [
      { file: dest.to_file, rank: dest.to_rank },
      { file: selected.file, rank: selected.rank },
    ];
    if (via) avoid.push(via);
    const pop = declineSquare(dest.to_file, dest.to_rank, avoid);
    promoPrompt = {
      file: dest.to_file,
      rank: dest.to_rank,
      popFile: pop?.file,
      popRank: pop?.rank,
      symbol: piece?.symbol || '?',
      promotionSymbol: piece?.promotion_symbol || `+${piece?.symbol || '?'}`,
      color: piece?.color || 'Black',
      via,
      matches,
    };
    highlights = [];
    log('Click the promoted piece, or the unpromoted choice to stay unpromoted');
  }

  async function selectPiece(file, rank, piece) {
    if (!piece || piece.color !== snapshot.turn) {
      log('Select one of your pieces', 'err');
      return;
    }
    selected = { file, rank };
    leg = null;
    promoPrompt = null;
    const epoch = beginNav();
    const res = await api.getMoves(file, rank);
    if (!navIsCurrent(epoch)) return;
    applyResult(res, true);
    if (!res.ok) {
      log(res.message, 'err');
      clearGesture();
      return;
    }
    pendingMoves = res.moves || [];
    highlights = firstMarks(pendingMoves);
    const twoStep = pendingMoves.some(hasVia);
    log(
      `Selected ${piece.symbol} at ${file},${rank} — ${pendingMoves.length} moves${twoStep ? ' (first step)' : ''}`,
    );
  }

  async function onCellClick({ file, rank }) {
    if (!snapshot) return;
    if (mode === 'play') {
      const ctrl = snapshot.turn === 'Black' ? blackController : whiteController;
      if (ctrl !== 'human') {
        log(`Side to move is controlled by ${ctrl}`, 'err');
        return;
      }
    }

    if (promoPrompt) {
      const via = promoPrompt.via;
      if (file === promoPrompt.file && rank === promoPrompt.rank) {
        const matches = promoPrompt.matches.filter((m) => m.promoted);
        promoPrompt = null;
        await sendMove(matches, true, via);
        return;
      }
      if (file === promoPrompt.popFile && rank === promoPrompt.popRank) {
        const matches = promoPrompt.matches.filter((m) => !m.promoted);
        promoPrompt = null;
        await sendMove(matches, false, via);
        return;
      }
      promoPrompt = null;
      highlights = leg ? secondMarks(pendingMoves, leg) : firstMarks(pendingMoves);
      return;
    }

    const piece = (snapshot.pieces || []).find(
      (p) => p.file === file && p.rank === rank,
    );

    if (!selected) {
      await selectPiece(file, rank, piece);
      return;
    }

    if (!leg && selected.file === file && selected.rank === rank) {
      clearGesture();
      return;
    }

    if (leg && selected.file === file && selected.rank === rank) {
      leg = null;
      highlights = firstMarks(pendingMoves);
      return;
    }

    if (
      piece &&
      piece.color === snapshot.turn &&
      !(leg && file === leg.file && rank === leg.rank)
    ) {
      await selectPiece(file, rank, piece);
      return;
    }

    if (leg) {
      if (file === leg.file && rank === leg.rank) {
        const stops = pendingMoves.filter(
          (m) => !hasVia(m) && m.to_file === file && m.to_rank === rank,
        );
        if (!stops.length) {
          log('This piece has to take a second step', 'err');
          return;
        }
        offerPromotion(stops, null);
        return;
      }
      const cont = pendingMoves.filter(
        (m) =>
          hasVia(m) &&
          m.via_file === leg.file &&
          m.via_rank === leg.rank &&
          m.to_file === file &&
          m.to_rank === rank,
      );
      if (!cont.length) {
        log('Not a legal second square', 'err');
        return;
      }
      offerPromotion(cont, { file: leg.file, rank: leg.rank });
      return;
    }

    const twoStep = pendingMoves.some(hasVia);
    if (twoStep) {
      const cont = pendingMoves.filter(
        (m) => hasVia(m) && m.via_file === file && m.via_rank === rank,
      );
      const stops = pendingMoves.filter(
        (m) => !hasVia(m) && m.to_file === file && m.to_rank === rank,
      );
      if (!cont.length && !stops.length) {
        log('Not a legal destination', 'err');
        return;
      }
      if (cont.length) {
        leg = { file, rank };
        highlights = secondMarks(pendingMoves, leg);
        log(
          stops.length
            ? 'Click this square again to stop, or choose the second square'
            : 'Choose the second square',
        );
        return;
      }
      offerPromotion(stops, null);
      return;
    }

    const matches = pendingMoves.filter(
      (m) => !hasVia(m) && m.to_file === file && m.to_rank === rank,
    );
    if (!matches.length) {
      log('Not a legal destination', 'err');
      return;
    }
    offerPromotion(matches, null);
  }

  async function stepBack() {
    const epoch = beginNav();
    const res = await api.back(1);
    if (!navIsCurrent(epoch)) return;
    clearGesture();
    applyResult(res, false, { clearSearch: true });
    if (res.ok) positionChanged();
  }

  async function stepForward() {
    const epoch = beginNav();
    const res = await api.forward(1);
    if (!navIsCurrent(epoch)) return;
    clearGesture();
    applyResult(res, false, { clearSearch: true });
    if (res.ok) positionChanged();
  }

  async function doGoto(ply) {
    const epoch = beginNav();
    const p = ply != null && ply !== '' ? Number(ply) : Number(gotoPly) || 0;
    gotoPly = p;
    const res = await api.gotoPly(p);
    if (!navIsCurrent(epoch)) return;
    clearGesture();
    applyResult(res, false, { clearSearch: true });
    if (res.ok) positionChanged();
  }

  async function playOnce() {
    const epoch = beginNav();
    const turn = snapshot?.turn || 'Black';
    const ctrl = turn === 'Black' ? blackController : whiteController;
    const agent = ctrl === 'human' ? 'mi' : ctrl;
    const res = await api.playAgent(agent, agentOptsFor(agent, turn));
    if (!navIsCurrent(epoch)) return;
    clearGesture();
    applyResult(res);
    if (res.ok) positionChanged();
  }

  async function startRun(black, white, label, modelsPair) {
    if (analysisActive) stopAnalysis();
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
    clearGesture();
    const epoch = beginNav();
    const res = await api.newGame();
    if (!navIsCurrent(epoch)) return;
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
      onclick={() => setMode('play')}>Play</button
    >
    <button
      class="mode-btn"
      class:active={mode === 'analysis'}
      onclick={() => setMode('analysis')}>Analysis</button
    >
    <button
      class="mode-btn"
      class:active={mode === 'debug'}
      onclick={() => setMode('debug')}>Debug</button
    >
    <button onclick={onNew}>New game</button>
    <button onclick={onSave}>Save</button>
    <span class="spacer"></span>
    {#if snapshot}
      <span
        >Turn: <strong>{snapshot.turn}</strong> · ply {snapshot.cursor}/{snapshot.timeline_len} · legal {snapshot.legal_move_count}</span
      >
    {/if}
  </div>

  <div class="main" class:analysis={mode === 'analysis'}>
    <div class="board-wrap">
      {#if promoPrompt && promoPrompt.popFile == null}
        <button onclick={() => {
          const { matches, via } = promoPrompt;
          promoPrompt = null;
          void sendMove(matches.filter((m) => !m.promoted), false, via);
        }}>Stay unpromoted</button>
      {/if}
      <Board
        pieces={boardPieces}
        selected={boardSelected}
        {highlights}
        arrows={mode === 'analysis' ? analysisArrows : []}
        choice={promoPrompt}
        {onCellClick}
      />
      <EvalSparkline
        series={evalSeries}
        cursor={snapshot?.cursor ?? 0}
        onGoto={doGoto}
      />
    </div>

    <div class="side">
      {#if mode === 'analysis'}
        <AnalysisPanel
          bind:active={analysisActive}
          {models}
          bind:model={analysisModel}
          bind:maxDepth={analysisMaxDepth}
          bind:qDepth={analysisQDepth}
          bind:cpuPercent={analysisCpuPercent}
          bind:lineCount={analysisLineCount}
          search={analysisSearch}
          elapsedMs={analysisElapsedMs}
          status={analysisStatus}
          onStart={startAnalysis}
          onStop={() => stopAnalysis()}
          onRestart={restartAnalysis}
        />
      {/if}

      {#if mode !== 'analysis'}
      <div class="panel games-panel">
        <h3>
          Games
          <span class="count">{filteredGames.length}/{games.length}</span>
        </h3>
        <p class="hint">Includes nested folders and tourney <code>slot*.json</code>.</p>
        <div class="row">
          <input
            class="game-filter"
            type="search"
            placeholder="Filter path…"
            bind:value={gameFilter}
          />
          <button type="button" onclick={refreshGames} title="Refresh list">↻</button>
        </div>
        <select
          class="game-list"
          size="10"
          bind:value={selectedGame}
          ondblclick={onLoad}
        >
          {#each gameGroups as [folder, items]}
            <optgroup label={folder}>
              {#each items as g}
                <option value={g}>{gameLabel(g)}</option>
              {/each}
            </optgroup>
          {/each}
        </select>
        <div class="row">
          <button type="button" onclick={onLoad} disabled={!selectedGame}>Load</button>
        </div>
        {#if selectedGame}
          <p class="hint path-hint" title={selectedGame}>{selectedGame}</p>
        {/if}
      </div>
      {/if}

      <div class="panel review-panel">
        <h3>
          Review
          <span class="count"
            >ply {snapshot?.cursor ?? 0}/{snapshot?.timeline_len ?? 0}</span
          >
        </h3>
        <div class="row">
          <button type="button" onclick={stepBack} disabled={!snapshot?.cursor}
            >◀ Back</button
          >
          <button
            type="button"
            onclick={stepForward}
            disabled={(snapshot?.cursor ?? 0) >= (snapshot?.timeline_len ?? 0)}
            >Forward ▶</button
          >
        </div>
        <div class="row">
          <input
            type="range"
            min="0"
            max={snapshot?.timeline_len || 0}
            bind:value={gotoPly}
            onchange={doGoto}
            disabled={!snapshot?.timeline_len}
          />
        </div>
        <div class="row">
          <input type="number" min="0" bind:value={gotoPly} style="width:5rem" />
          <button type="button" onclick={doGoto}>Goto ply</button>
        </div>

        {#if recordedHasData}
          <p class="hint">Scores are black-absolute (B+ / W+).</p>
          {#if hasRecordedEval(snapshot.recorded?.last)}
            {@const last = snapshot.recorded.last}
            <div class="recorded-block">
              <div class="recorded-head">
                Played · ply {last.ply} · {last.side}
              </div>
              <div class="recorded-move">{last.label}</div>
              <div class="eval-row">
                <span
                  >static
                  <strong class={scoreClass(last.static_eval)}
                    >{formatBlackAbs(last.static_eval)}</strong
                  ></span
                >
                <span>→</span>
                <span
                  >search
                  <strong class={scoreClass(last.eval)}
                    >{formatBlackAbs(last.eval)}</strong
                  ></span
                >
                {#if last.nodes != null}
                  <span class="meta">{last.nodes} nodes</span>
                {/if}
              </div>
            </div>
          {/if}
          {#if hasRecordedEval(snapshot.recorded?.next)}
            {@const next = snapshot.recorded.next}
            <div class="recorded-block next">
              <div class="recorded-head">
                Next · ply {next.ply} · {next.side}
              </div>
              <div class="recorded-move">{next.label}</div>
              <div class="eval-row">
                <span
                  >static
                  <strong class={scoreClass(next.static_eval)}
                    >{formatBlackAbs(next.static_eval)}</strong
                  ></span
                >
                <span>→</span>
                <span
                  >search
                  <strong class={scoreClass(next.eval)}
                    >{formatBlackAbs(next.eval)}</strong
                  ></span
                >
                {#if next.nodes != null}
                  <span class="meta">{next.nodes} nodes</span>
                {/if}
              </div>
            </div>
          {/if}
        {:else if (snapshot?.timeline_len ?? 0) > 0}
          <p class="hint">No recorded evals on this game’s moves.</p>
        {/if}

        {#if mode === 'debug'}
          <div class="row">
            <button type="button" onclick={onSuggest}>Suggest mi</button>
            <button type="button" onclick={playOnce}>Play mi here</button>
          </div>
        {/if}
      </div>

      {#if mode === 'play'}
        <div class="panel">
          <h3>Controllers</h3>
          <div class="row">
            <label for="black-controller">Black</label>
            <select id="black-controller" bind:value={blackController}>
              <option value="human">Human</option>
              <option value="mi">mi</option>
              <option value="random">random</option>
              <option value="royal">royal</option>
              <option value="ab">Alpha-beta / NNUE</option>
            </select>
          </div>
          <div class="row">
            <label for="white-controller">White</label>
            <select id="white-controller" bind:value={whiteController}>
              <option value="human">Human</option>
              <option value="mi">mi</option>
              <option value="random">random</option>
              <option value="royal">royal</option>
              <option value="ab">Alpha-beta / NNUE</option>
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
          <h3>Engine models</h3>
          <p class="hint">Select Alpha-beta / NNUE above, then choose a model for each side. Use Human for your side to play against an engine.</p>
          <div class="row">
            <label for="black-model">Black model</label>
            <select id="black-model" bind:value={blackAbModel}>
              {#each models as m}
                <option value={m}>{m}</option>
              {/each}
            </select>
          </div>
          <div class="row">
            <label for="white-model">White model</label>
            <select id="white-model" bind:value={whiteAbModel}>
              {#each models as m}
                <option value={m}>{m}</option>
              {/each}
            </select>
            <button onclick={refreshModels} title="Refresh models">↻</button>
          </div>
          <div class="row">
            <label for="play-depth">Depth</label>
            <input
              id="play-depth"
              type="number"
              min="1"
              max="64"
              bind:value={abDepth}
              style="width:4rem"
            />
          </div>
          <div class="row">
            <label for="play-q-depth">Q-depth</label>
            <input
              id="play-q-depth"
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
            <label for="play-time-ms">Time ms</label>
            <input
              id="play-time-ms"
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
      {/if}

      {#if mode !== 'analysis'}
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
      {/if}

      {#if mode !== 'analysis'}
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
      {/if}
    </div>
  </div>

  {#if mode !== 'analysis'}
  <div class="log">
    {#each logLines as line}
      <div class={line.includes('ERROR') ? 'err' : 'ok'}>{line}</div>
    {:else}
      <div>Log: moves, agents, errors…</div>
    {/each}
  </div>
  {/if}
</div>
