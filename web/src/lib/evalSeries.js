// Colors identify the evaluating agent, independently of which side is ahead.
const STYLES = [
  { side: 'Black', label: 'Black agent', color: '#1d4ed8', dashed: false },
  { side: 'White', label: 'White agent', color: '#b45309', dashed: true },
  { side: null, label: 'Eval trace', color: '#516057', dashed: false },
];

export function evalLines(series) {
  const points = series?.points || [];
  return STYLES.map(style => ({
    ...style,
    label: style.side ? style.label : series?.source === 'eval_trace' ? 'Eval trace' : 'Unattributed',
    // Use the recorded mover, not ply parity: games can start with White and
    // samples can be missing. Offline traces have no per-agent attribution.
    points: points.filter(point => (point.side ?? null) === style.side),
  })).filter(line => line.points.length);
}

export function nearestEval(points, ply) {
  let best = null;
  for (const point of points) {
    if (best === null || Math.abs(point.ply - ply) < Math.abs(best.ply - ply)) best = point;
  }
  return best;
}
