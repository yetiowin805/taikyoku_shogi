import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

// Execute the real component handlers with API stubs. Rendering and effects are
// deliberately excluded; these tests cover gesture state and request routing.
const source = readFileSync(new URL('../src/App.svelte', import.meta.url), 'utf8')
  .split('<script>')[1].split('</script>')[0].replace(/^\s*import .*;$/gm, '');
function app() {
  const sent = [];
  const snapshot = { turn: 'Black', cursor: 1, pieces: [] };
  const result = { ok: true, snapshot, message: 'ok' };
  const api = Object.fromEntries(['newGame', 'loadGame', 'back', 'forward', 'gotoPly', 'playAgent']
    .map(name => [name, async () => result]));
  api.applyMove = async body => { sent.push(body); return result; };
  const context = vm.createContext({ api, $state: x => x,
    $derived: Object.assign(x => x, { by: () => undefined }), $effect: () => {} });
  vm.runInContext(source, context);
  const run = code => vm.runInContext(code, context);
  const get = code => JSON.parse(run(`JSON.stringify(${code})`));
  run(`snapshot = {turn: 'Black', pieces: [{file: 10, rank: 10, color: 'Black', symbol: 'P', promotion_symbol: 'G'}]};
    boardPieces = snapshot.pieces;
    selected = {file: 10, rank: 10};
    pendingMoves = [{to_file: 10, to_rank: 11, promoted: false}, {to_file: 10, to_rank: 11, promoted: true}];
    highlights = firstMarks(pendingMoves);`);
  return { run, get, sent };
}
function cleared(a) {
  assert.deepEqual(a.get('[selected, highlights, pendingMoves, leg, promoPrompt]'), [null, [], [], null, null]);
}

test('deselecting and completing a move clear the whole gesture', async () => {
  const a = app();
  await a.run('onCellClick({file: 10, rank: 10})');
  cleared(a);
  const b = app();
  await b.run('sendMove(pendingMoves.filter(m => !m.promoted), false, null)');
  assert.equal(b.sent.length, 1);
  assert.equal(b.sent[0].direct, true);
  cleared(b);
});

for (const action of ['onNew()', 'onLoad()', 'stepBack()', 'stepForward()', 'doGoto(1)', 'playOnce()', 'runAgentIfNeeded()']) {
  test(`${action} clears gestures and updates the snapshot`, async () => {
    const a = app();
    a.run("selectedGame = 'test'; autoPlay = true; blackController = 'mi'; leg = {file: 10, rank: 11}; promoPrompt = {};");
    await a.run(action);
    cleared(a);
    assert.equal(a.get('snapshot.cursor'), 1);
  });
}

test('dismissal consumes the click and restores direct or second-leg highlights', async () => {
  for (const twoStep of [false, true]) {
    const a = app();
    if (twoStep) a.run(`leg = {file: 11, rank: 10}; pendingMoves = pendingMoves.map(m => ({...m, via_file: 11, via_rank: 10}));`);
    const expected = a.get('leg ? secondMarks(pendingMoves, leg) : firstMarks(pendingMoves)');
    a.run('offerPromotion(pendingMoves, leg)');
    assert.equal(a.get('promoPrompt.promotionSymbol'), 'G');
    // The source square would otherwise deselect or back out of the first leg.
    await a.run('onCellClick({file: 10, rank: 10})');
    assert.equal(a.get('promoPrompt'), null);
    assert.deepEqual(a.get('highlights'), expected);
    assert.deepEqual(a.get('selected'), {file: 10, rank: 10});
    assert.equal(Boolean(a.get('leg')), twoStep);
    assert.equal(a.sent.length, 0);
  }
});

test('promotion choices send the selected path and promotion flag', async () => {
  for (const promote of [true, false]) {
    const a = app();
    a.run('pendingMoves = pendingMoves.map(m => ({...m, via_file: 11, via_rank: 10})); offerPromotion(pendingMoves, {file: 11, rank: 10})');
    await a.run(promote ? 'onCellClick({file: promoPrompt.file, rank: promoPrompt.rank})' : 'onCellClick({file: promoPrompt.popFile, rank: promoPrompt.popRank})');
    assert.equal(a.sent[0].promote, promote);
    assert.equal(a.sent[0].via_file, 11);
    assert.equal(a.sent[0].direct, false);
    cleared(a);
  }
});

test('decline square avoids pieces and edges, including crowded positions', () => {
  const a = app();
  a.run('boardPieces = [{file: 2, rank: 1}, {file: 1, rank: 2}]');
  const square = a.get('declineSquare(1, 1, [{file: 1, rank: 1}])');
  assert.ok(square.file >= 1 && square.rank >= 1);
  assert.ok(square.file + square.rank > 3);
  a.run('boardPieces = Array.from({length: 1296}, (_, i) => ({file: i % 36 + 1, rank: Math.floor(i / 36) + 1}))');
  assert.equal(a.get('declineSquare(1, 1, [])'), null);
});
