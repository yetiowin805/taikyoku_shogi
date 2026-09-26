<script>
  /**
   * Canvas board: shogi-style coords (file 1 rightmost, rank 1 top).
   * Props use the same numbering as the Rust API.
   *
   * Squares and pieces sit on the base canvas. Arrows sit on an overlay so
   * analysis updates do not repaint every piece.
   */
  let {
    pieces = [],
    selected = null,
    highlights = [],
    arrows = [],
    onCellClick = () => {},
  } = $props();

  let base;
  let overlay;
  const N = 36;
  const CELL = 22;
  const PAD = 18;
  const W = PAD + N * CELL + 4;
  const H = PAD + N * CELL + 4;
  const FONT = `${Math.floor(CELL * 0.45)}px sans-serif`;
  const FONT_BOLD = `bold ${Math.floor(CELL * 0.45)}px sans-serif`;

  function cellIndex(file, rank) {
    return (file - 1) * N + (rank - 1);
  }

  function squareCenter(file, rank) {
    return {
      x: PAD + (N - file) * CELL + CELL / 2,
      y: PAD + (rank - 1) * CELL + CELL / 2,
    };
  }

  function drawArrow(ctx, arrow, index) {
    const from = squareCenter(arrow.from_file, arrow.from_rank);
    const to = squareCenter(arrow.to_file, arrow.to_rank);
    const dx = to.x - from.x;
    const dy = to.y - from.y;
    const length = Math.hypot(dx, dy);
    if (length < 1) return;
    const ux = dx / length;
    const uy = dy / length;
    const head = arrow.best ? 10 : 8;
    const width = arrow.best ? 5 : 3;
    const color = arrow.best ? '#18a36b' : '#3578c5';

    ctx.save();
    ctx.globalAlpha = arrow.best ? 0.9 : Math.max(0.38, 0.7 - index * 0.1);
    ctx.strokeStyle = color;
    ctx.fillStyle = color;
    ctx.lineWidth = width;
    ctx.lineCap = 'round';
    ctx.beginPath();
    ctx.moveTo(from.x + ux * 4, from.y + uy * 4);
    ctx.lineTo(to.x - ux * (head * 0.65), to.y - uy * (head * 0.65));
    ctx.stroke();

    const baseX = to.x - ux * head;
    const baseY = to.y - uy * head;
    const px = -uy;
    const py = ux;
    ctx.beginPath();
    ctx.moveTo(to.x, to.y);
    ctx.lineTo(baseX + px * head * 0.55, baseY + py * head * 0.55);
    ctx.lineTo(baseX - px * head * 0.55, baseY - py * head * 0.55);
    ctx.closePath();
    ctx.fill();

    ctx.beginPath();
    ctx.arc(from.x, from.y, width * 0.8, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  }

  function drawBase() {
    if (!base) return;
    const ctx = base.getContext('2d');
    const occupied = new Array(N * N);
    for (const piece of pieces) {
      if (piece.file >= 1 && piece.file <= N && piece.rank >= 1 && piece.rank <= N) {
        occupied[cellIndex(piece.file, piece.rank)] = piece;
      }
    }
    const marks = new Array(N * N);
    for (const mark of highlights) {
      if (mark.file >= 1 && mark.file <= N && mark.rank >= 1 && mark.rank <= N) {
        marks[cellIndex(mark.file, mark.rank)] = mark;
      }
    }

    ctx.clearRect(0, 0, W, H);
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    let bold = null;

    // ranks top→bottom = 1..36, files right→left = 1..36 in shogi display
    // Our canvas x increases left→right; file 36 is leftmost, file 1 rightmost.
    for (let rank = 1; rank <= N; rank++) {
      for (let file = 1; file <= N; file++) {
        const col = N - file; // file 36 → col 0
        const row = rank - 1;
        const x = PAD + col * CELL;
        const y = PAD + row * CELL;
        const dark = (col + row) % 2 === 0;
        ctx.fillStyle = dark ? '#d2c7a8' : '#efe9d8';
        ctx.fillRect(x, y, CELL, CELL);

        const isSel = selected && selected.file === file && selected.rank === rank;
        const hi = marks[cellIndex(file, rank)];
        if (isSel) {
          ctx.fillStyle = 'rgba(47, 93, 80, 0.45)';
          ctx.fillRect(x, y, CELL, CELL);
        } else if (hi) {
          ctx.fillStyle = hi.capture
            ? 'rgba(180, 60, 40, 0.4)'
            : 'rgba(60, 120, 200, 0.35)';
          ctx.fillRect(x, y, CELL, CELL);
        }

        const p = occupied[cellIndex(file, rank)];
        if (p) {
          const nextBold = !!p.promoted;
          if (nextBold !== bold) {
            ctx.font = nextBold ? FONT_BOLD : FONT;
            bold = nextBold;
          }
          ctx.fillStyle = p.color === 'Black' ? '#111' : '#b33';
          const label = (p.promoted ? '+' : '') + (p.symbol || '?');
          ctx.fillText(label, x + CELL / 2, y + CELL / 2);
        }
      }
    }

    ctx.strokeStyle = '#8a8170';
    ctx.strokeRect(PAD, PAD, N * CELL, N * CELL);

    ctx.fillStyle = '#555';
    ctx.font = '9px sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'alphabetic';
    for (let file = 1; file <= N; file += 6) {
      const col = N - file;
      ctx.fillText(String(file), PAD + col * CELL + CELL / 2, PAD - 4);
    }
    ctx.textAlign = 'right';
    for (let rank = 1; rank <= N; rank += 6) {
      const row = rank - 1;
      ctx.fillText(String(rank), PAD - 3, PAD + row * CELL + CELL / 2 + 3);
    }
  }

  function drawArrows() {
    if (!overlay) return;
    const ctx = overlay.getContext('2d');
    ctx.clearRect(0, 0, W, H);
    [...arrows]
      .sort((a, b) => Number(a.best) - Number(b.best))
      .forEach((arrow, index) => drawArrow(ctx, arrow, index));
  }

  $effect(() => {
    pieces;
    selected;
    highlights;
    drawBase();
  });

  $effect(() => {
    arrows;
    drawArrows();
  });

  function handleClick(ev) {
    const rect = overlay.getBoundingClientRect();
    const scaleX = overlay.width / rect.width;
    const scaleY = overlay.height / rect.height;
    const mx = (ev.clientX - rect.left) * scaleX;
    const my = (ev.clientY - rect.top) * scaleY;
    const col = Math.floor((mx - PAD) / CELL);
    const row = Math.floor((my - PAD) / CELL);
    if (col < 0 || col >= N || row < 0 || row >= N) return;
    const file = N - col;
    const rank = row + 1;
    onCellClick({ file, rank });
  }
</script>

<div class="board-stack">
  <canvas bind:this={base} width={W} height={H}></canvas>
  <canvas
    bind:this={overlay}
    width={W}
    height={H}
    onclick={handleClick}
  ></canvas>
</div>

<style>
  .board-stack {
    position: relative;
    width: fit-content;
    max-width: 100%;
  }

  canvas {
    display: block;
    max-width: 100%;
    height: auto;
  }

  canvas:last-child {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    cursor: pointer;
  }
</style>
