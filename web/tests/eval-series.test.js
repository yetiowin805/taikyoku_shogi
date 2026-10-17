import assert from 'node:assert/strict';
import test from 'node:test';
import { evalLines, nearestEval } from '../src/lib/evalSeries.js';

test('disagreeing agents have independent trajectories with distinct styles', () => {
  const points = [
    { ply: 1, eval: 800, side: 'Black' },
    { ply: 2, eval: -700, side: 'White' },
    { ply: 3, eval: 900, side: 'Black' },
    { ply: 4, eval: -600, side: 'White' },
  ];
  const [black, white] = evalLines({ source: 'recorded', points });
  assert.deepEqual(black.points.map(p => p.eval), [800, 900]);
  assert.deepEqual(white.points.map(p => p.eval), [-700, -600]);
  assert.notEqual(black.color, white.color);
  assert.notEqual(black.dashed, white.dashed);
  assert.equal(nearestEval(black.points, 4).ply, 3);
  assert.equal(nearestEval(white.points, 4).ply, 4);
});

test('White-first starts and missing scores do not change agent attribution', () => {
  const points = [
    { ply: 1, eval: 100, side: 'White' },
    { ply: 4, eval: 200, side: 'Black' },
    { ply: 7, eval: 300, side: 'White' },
  ];
  const [black, white] = evalLines({ source: 'recorded', points });
  assert.deepEqual(black.points.map(p => p.ply), [4]);
  assert.deepEqual(white.points.map(p => p.ply), [1, 7]);
  assert.equal(nearestEval(black.points, 0).ply, 4);
  assert.equal(nearestEval(white.points, 100).ply, 7);
});

test('offline traces and older samples stay unattributed, including ply zero', () => {
  const points = [{ ply: 0, eval: 50 }, { ply: 16, eval: -30 }];
  const [trace] = evalLines({ source: 'eval_trace', points });
  assert.equal(trace.label, 'Eval trace');
  assert.equal(trace.side, null);
  assert.deepEqual(trace.points, points);
  const [legacy] = evalLines({ source: 'recorded', points });
  assert.equal(legacy.label, 'Unattributed');
});

test('empty and one-sided games omit empty legend entries', () => {
  assert.deepEqual(evalLines(null), []);
  assert.deepEqual(evalLines({ points: [] }), []);
  assert.equal(nearestEval([], 0), null);
  const [line] = evalLines({ points: [{ ply: 2, eval: 0, side: 'White' }] });
  assert.equal(line.label, 'White agent');
  assert.equal(nearestEval(line.points, 10).eval, 0);
});
