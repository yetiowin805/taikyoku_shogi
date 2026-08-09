<script>
  /**
   * Interactive black-absolute eval chart. Click / drag to scrub to a ply.
   */
  let {
    series = null,
    cursor = 0,
    onGoto = () => {},
  } = $props();

  const W = 720;
  const H = 128;
  const PAD = { l: 40, r: 10, t: 12, b: 20 };

  let hoverPly = $state(null);
  let dragging = $state(false);

  let points = $derived(series?.points || []);
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

  let zeroOffsetPct = $derived.by(() => {
    if (!bounds) return 50;
    const { minE, maxE } = bounds;
    const span = maxE - minE || 1;
    return ((maxE - 0) / span) * 100;
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

  function pathD() {
    if (!bounds || points.length < 1) return '';
    return points
      .map((p, i) => `${i === 0 ? 'M' : 'L'}${xOf(p.ply).toFixed(1)},${yOf(p.eval).toFixed(1)}`)
      .join(' ');
  }

  function areaD() {
    if (!bounds || points.length < 1) return '';
    const y0 = yOf(0);
    const first = points[0];
    const last = points[points.length - 1];
    let d = `M${xOf(first.ply).toFixed(1)},${y0.toFixed(1)}`;
    for (const p of points) {
      d += `L${xOf(p.ply).toFixed(1)},${yOf(p.eval).toFixed(1)}`;
    }
    d += `L${xOf(last.ply).toFixed(1)},${y0.toFixed(1)}Z`;
    return d;
  }

  function plyAtClientX(clientX, el) {
    if (!bounds || !el) return 0;
    const rect = el.getBoundingClientRect();
    const x = ((clientX - rect.left) / rect.width) * W;
    const iw = W - PAD.l - PAD.r;
    const t = Math.min(1, Math.max(0, (x - PAD.l) / iw));
    return Math.round(t * bounds.maxP);
  }

  function nearestEval(ply) {
    if (!points.length) return null;
    let best = points[0];
    let bestD = Math.abs(best.ply - ply);
    for (const p of points) {
      const d = Math.abs(p.ply - ply);
      if (d < bestD) {
        best = p;
        bestD = d;
      }
    }
    return best;
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

  let cursorEval = $derived(nearestEval(cursor));
  let hoverEval = $derived(hoverPly != null ? nearestEval(hoverPly) : null);
  let labelEval = $derived(hoverEval || cursorEval);
  let gradId = $derived(`evalGrad-${series?.game_id || 'x'}`);
</script>

{#if series && points.length && bounds}
  <div class="eval-spark">
    <div class="eval-meta">
      <span>
        Eval ({series.source}) · ply {labelEval?.ply ?? cursor}
        {#if labelEval != null}
          ·
          <strong class:pos={labelEval.eval >= 0} class:neg={labelEval.eval < 0}
            >{formatAbs(labelEval.eval)}</strong
          >
        {/if}
      </span>
      <span class="hint">click / drag to scrub · B+ / W+</span>
    </div>
    <svg
      viewBox="0 0 {W} {H}"
      role="img"
      aria-label="Evaluation over the game"
      onpointerdown={(e) => {
        dragging = true;
        e.currentTarget.setPointerCapture(e.pointerId);
        onPointer(e, true);
      }}
      onpointermove={(e) => onPointer(e, dragging)}
      onpointerup={() => (dragging = false)}
      onpointerleave={() => {
        if (!dragging) hoverPly = null;
      }}
    >
      <defs>
        <linearGradient
          id={gradId}
          x1="0"
          y1={yOf(bounds.maxE)}
          x2="0"
          y2={yOf(bounds.minE)}
          gradientUnits="userSpaceOnUse"
        >
          <stop offset="0%" stop-color="#1a6b3c" stop-opacity="0.38" />
          <stop offset="{zeroOffsetPct}%" stop-color="#1a6b3c" stop-opacity="0.04" />
          <stop offset="{zeroOffsetPct}%" stop-color="#9b2c2c" stop-opacity="0.04" />
          <stop offset="100%" stop-color="#9b2c2c" stop-opacity="0.38" />
        </linearGradient>
      </defs>
      <path class="area" d={areaD()} fill={`url(#${gradId})`} />
      <line class="zero" x1={PAD.l} x2={W - PAD.r} y1={yOf(0)} y2={yOf(0)} />
      {#each focus as fp}
        <line class="focus" x1={xOf(fp)} x2={xOf(fp)} y1={PAD.t} y2={H - PAD.b} />
      {/each}
      <path class="line" d={pathD()} />
      <line class="cursor" x1={xOf(cursor)} x2={xOf(cursor)} y1={PAD.t} y2={H - PAD.b} />
      {#if cursorEval}
        <circle class="dot" cx={xOf(cursorEval.ply)} cy={yOf(cursorEval.eval)} r="3.5" />
      {/if}
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
    gap: 0.5rem;
    font-size: 0.8rem;
    margin-bottom: 0.2rem;
  }
  .eval-meta .hint {
    color: #5a5346;
  }
  .eval-meta .pos {
    color: #1a5c3a;
  }
  .eval-meta .neg {
    color: #8b2e2e;
  }
  svg {
    display: block;
    width: 100%;
    height: auto;
    background: #faf7ef;
    border: 1px solid #c9c2b0;
    cursor: crosshair;
    touch-action: none;
  }
  .area {
    stroke: none;
  }
  .line {
    fill: none;
    stroke: #2f5d50;
    stroke-width: 1.6;
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
    fill: #2f5d50;
    stroke: #faf7ef;
    stroke-width: 1;
  }
  .axis {
    fill: #5a5346;
    font-size: 9px;
    font-family: "IBM Plex Mono", ui-monospace, monospace;
  }
</style>
