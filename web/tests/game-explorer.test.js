import assert from 'node:assert/strict';
import test from 'node:test';
import { explorerEntries, gameName, parentFolder } from '../src/lib/gameExplorer.js';

const games = ['games/game10.json', 'games/game2.json', 'data/raw/tourney/slot1.json', 'root.json'];

test('browsing shows immediate folders with recursive game counts', () => {
  assert.deepEqual(explorerEntries(games), [
    { kind: 'folder', name: 'data', path: 'data', count: 1 },
    { kind: 'folder', name: 'games', path: 'games', count: 2 },
    { kind: 'file', name: 'root.json', path: 'root.json', parent: '' },
  ]);
  assert.deepEqual(explorerEntries(games, 'games').map(entry => entry.name), ['game2.json', 'game10.json']);
  assert.equal(explorerEntries(games, 'game').length, 0);
  assert.equal(explorerEntries(games, 'data')[0].path, 'data/raw');
});

test('search covers the entire library and includes folder paths', () => {
  assert.deepEqual(explorerEntries(games, 'games', ' TOURNEY ').map(entry => entry.path), ['data/raw/tourney/slot1.json']);
  assert.equal(explorerEntries(games, '', 'missing').length, 0);
  assert.equal(explorerEntries(games, '', 'game')[0].name, 'game2.json');
});

test('Windows separators are navigable while load paths stay unchanged', () => {
  const path = 'data\\raw\\slot2.json';
  assert.equal(explorerEntries([path], 'data/raw')[0].path, path);
  assert.equal(explorerEntries([path], '', 'data/raw')[0].path, path);
  assert.equal(gameName(path), 'slot2.json');
  assert.equal(parentFolder(path), 'data/raw');
  assert.equal(parentFolder('data'), '');
});

test('empty and removed folders return no stale entries', () => {
  assert.deepEqual(explorerEntries([]), []);
  assert.deepEqual(explorerEntries(['games/new.json'], 'deleted'), []);
});
