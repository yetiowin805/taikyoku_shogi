<script>
  import { evalLines, nearestEval } from './evalSeries.js';
  /**
   * Interactive black-absolute eval chart. Click / drag to scrub to a ply.
   */
  let {
    series = null,
    cursor = 0,
    onGoto = () => {},
  } = $props();

  const W = 720;
  const H = 144;
  const PAD = { l: 62, r: 10, t: 12, b: 20 };

  let hoverPly = $state(null);
  let dragging = $state(false);

  let points = $derived(series?.points || []);
  let lines = $derived(evalLines(series));
  let focus = $derived(series?.focus_plies || []);

  let bounds = $derived.by(() => {
    if (!points.length) return null;
    let minE = Infinity;
    let maxE = -Infinity;
    let maxP = 0;
    for (const p of points) {
      if (p.eval < minE) minE = p.eval;
      if (p.eval > maxE) maxE = p.eval;
      if (p.ply > maxP) maxP = p.ply;
    }
    if (minE === maxE) {
      minE -= 100;
      maxE += 100;
    }
    // Keep zero in view when the series crosses / is near it.
    if (minE > 0) minE = 0;
    if (maxE < 0) maxE = 0;
    const pad = (maxE - minE) * 0.06 || 50;
    return { minE: minE - pad, maxE: maxE + pad, maxP: Math.max(maxP, 1) };
  });

  function xOf(ply) {
    const { maxP } = bounds;
    const iw = W - PAD.l - PAD.r;
    return PAD.l + (ply / maxP) * iw;
  }

  function yOf(ev) {
    const { minE, maxE } = bounds;
    const ih = H - PAD.t - PAD.b;
    return PAD.t + ((maxE - ev) / (maxE - minE)) * ih;
  }

  function pathD(linePoints) {
    if (!bounds || linePoints.length < 1) return '';
    return linePoints
      .map((p, i) => `${i === 0 ? 'M' : 'L'}${xOf(p.ply).toFixed(1)},${yOf(p.eval).toFixed(1)}`)
      .join(' ');
  }

  function plyAtClientX(clientX, el) {
    if (!bounds || !el) return 0;
    const rect = el.getBoundingClientRect();
    const x = ((clientX - rect.left) / rect.width) * W;
    const iw = W - PAD.l - PAD.r;
    const t = Math.min(1, Math.max(0, (x - PAD.l) / iw));
    return Math.round(t * bounds.maxP);
  }

  function formatAbs(v) {
    if (v == null || Number.isNaN(v)) return '—';
    const n = Math.round(v);
    if (n > 0) return `B+${n}`;
    if (n < 0) return `W+${Math.abs(n)}`;
    return '0';
  }

  function onPointer(e, commit) {
    const svg = e.currentTarget;
    const ply = plyAtClientX(e.clientX, svg);
    hoverPly = ply;
    if (commit) onGoto(ply);
  }

  let markerPly = $derived(dragging && hoverPly != null ? hoverPly : cursor);
  let selectedPly = $derived(hoverPly ?? markerPly);
  let readings = $derived(lines.map(line => ({
    ...line,
    selected: nearestEval(line.points, selectedPly),
    marker: nearestEval(line.points, markerPly),
  })));
</script>

{#if series && points.length && bounds}
  <div class="eval-spark">
    <div class="eval-meta">
      <span>Eval ({series.source}) · ply {selectedPly}</span>
      <span class="hint">click / drag to scrub · B+ / W+ = side ahead</span>
    </div>
    <div class="eval-legend">
      {#each readings as line (line.label)}
        <span class="legend-item" style:color={line.color}>
          <span class="swatch" class:dashed={line.dashed}></span>
          <strong>{line.label}</strong>
          <span>{formatAbs(line.selected.eval)} <span class="sample-ply">@ ply {line.selected.ply}</span></span>
        </span>
      {/each}
    </div>
    <svg
      viewBox="0 0 {W} {H}"
      role="img"
      aria-label="Evaluation over the game, with separate lines for each evaluating agent. Positive scores favor Black; negative scores favor White."
      onpointerdown={(e) => {
        dragging = true;
        e.currentTarget.setPointerCapture(e.pointerId);
        onPointer(e, false);
      }}
      onpointermove={(e) => {
        onPointer(e, false);
      }}
      onpointerup={(e) => {
        if (dragging) onPointer(e, true);
        dragging = false;
      }}
      onpointercancel={() => {
        dragging = false;
        hoverPly = null;
      }}
      onpointerleave={() => {
        if (!dragging) hoverPly = null;
      }}
    >
      <line class="zero" x1={PAD.l} x2={W - PAD.r} y1={yOf(0)} y2={yOf(0)} />
      {#each focus as fp}
        <line class="focus" x1={xOf(fp)} x2={xOf(fp)} y1={PAD.t} y2={H - PAD.b} />
      {/each}
      {#each lines as line (line.label)}
        <path class="line" d={pathD(line.points)} stroke={line.color} stroke-dasharray={line.dashed ? '6 4' : undefined}>
          <title>{line.label}</title>
        </path>
        {#if line.points.length === 1}
          <circle class="dot" cx={xOf(line.points[0].ply)} cy={yOf(line.points[0].eval)} r="3" fill={line.color} />
        {/if}
      {/each}
      <line class="cursor" x1={xOf(markerPly)} x2={xOf(markerPly)} y1={PAD.t} y2={H - PAD.b} />
      {#each readings as line (line.label)}
        <circle class="dot" cx={xOf(line.marker.ply)} cy={yOf(line.marker.eval)} r="3.5" fill={line.color} />
      {/each}
      <text class="axis" x={4} y={yOf(bounds.maxE) + 4}>{formatAbs(bounds.maxE)}</text>
      <text class="axis" x={4} y={yOf(bounds.minE)}>{formatAbs(bounds.minE)}</text>
      <text class="axis" x={PAD.l} y={H - 4}>0</text>
      <text class="axis" x={W - PAD.r - 28} y={H - 4}>{bounds.maxP}</text>
    </svg>
  </div>
{/if}

<style>
  .eval-spark {
    margin-top: 0.55rem;
    width: 100%;
  }
  .eval-meta {
    display: flex;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 0.5rem;
    font-size: 0.8rem;
    margin-bottom: 0.2rem;
  }
  .eval-meta .hint {
    color: #5a5346;
  }
  .eval-legend {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem 1.2rem;
    margin-bottom: 0.35rem;
    font-size: 0.8rem;
  }
  .legend-item {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }
  .swatch {
    width: 1.4rem;
    border-top: 2px solid currentColor;
  }
  .swatch.dashed { border-top-style: dashed; }
  .sample-ply { color: #5a5346; font-size: 0.7rem; }
  svg {
    display: block;
    width: 100%;
    height: auto;
    background: #faf7ef;
    border: 1px solid #c9c2b0;
    cursor: crosshair;
    touch-action: none;
  }
  .line {
    fill: none;
    stroke-width: 2;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .zero {
    stroke: #b0a890;
    stroke-width: 1;
    stroke-dasharray: 3 3;
  }
  .focus {
    stroke: #c47a2c;
    stroke-width: 1;
    opacity: 0.55;
  }
  .cursor {
    stroke: #1a1a1a;
    stroke-width: 1.2;
  }
  .dot {
    stroke: #faf7ef;
    stroke-width: 1;
  }
  .axis {
    fill: #5a5346;
    font-size: 9px;
    font-family: "IBM Plex Mono", ui-monospace, monospace;
  }
</style>
