<script>
  /**
   * Canvas board: shogi-style coords (file 1 rightmost, rank 1 top).
   * Props use the same numbering as the Rust API.
   */
  let {
    pieces = [],
    selected = null,
    highlights = [],
    onCellClick = () => {},
  } = $props();

  let canvas;
  const N = 36;
  const CELL = 22;
  const PAD = 18;
  const W = PAD + N * CELL + 4;
  const H = PAD + N * CELL + 4;

  function pieceAt(file, rank) {
    return pieces.find((p) => p.file === file && p.rank === rank);
  }

  function draw() {
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    ctx.clearRect(0, 0, W, H);

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
        const hi = highlights.find((h) => h.file === file && h.rank === rank);
        if (isSel) {
          ctx.fillStyle = 'rgba(47, 93, 80, 0.45)';
          ctx.fillRect(x, y, CELL, CELL);
        } else if (hi) {
          ctx.fillStyle = hi.capture
            ? 'rgba(180, 60, 40, 0.4)'
            : 'rgba(60, 120, 200, 0.35)';
          ctx.fillRect(x, y, CELL, CELL);
        }

        const p = pieceAt(file, rank);
        if (p) {
          ctx.fillStyle = p.color === 'Black' ? '#111' : '#b33';
          ctx.font = p.promoted
            ? `bold ${Math.floor(CELL * 0.45)}px sans-serif`
            : `${Math.floor(CELL * 0.45)}px sans-serif`;
          ctx.textAlign = 'center';
          ctx.textBaseline = 'middle';
          const label = (p.promoted ? '+' : '') + (p.symbol || '?');
          ctx.fillText(label, x + CELL / 2, y + CELL / 2);
        }
      }
    }

    ctx.strokeStyle = '#8a8170';
    ctx.strokeRect(PAD, PAD, N * CELL, N * CELL);

    // light axis labels every 6
    ctx.fillStyle = '#555';
    ctx.font = '9px sans-serif';
    ctx.textAlign = 'center';
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

  $effect(() => {
    pieces;
    selected;
    highlights;
    draw();
  });

  function handleClick(ev) {
    const rect = canvas.getBoundingClientRect();
    const scaleX = canvas.width / rect.width;
    const scaleY = canvas.height / rect.height;
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

<canvas
  bind:this={canvas}
  width={W}
  height={H}
  onclick={handleClick}
  style="image-rendering: pixelated; max-width: 100%; cursor: pointer;"
></canvas>
