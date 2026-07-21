const BASE = '/api';

async function req(path, options = {}) {
  const res = await fetch(`${BASE}${path}`, {
    headers: { 'Content-Type': 'application/json', ...(options.headers || {}) },
    ...options,
  });
  const data = await res.json();
  return data;
}

export async function getState() {
  return req('/state');
}

export async function newGame() {
  return req('/new', { method: 'POST', body: '{}' });
}

export async function listGames() {
  return req('/list');
}

export async function loadGame(filename) {
  return req('/load', { method: 'POST', body: JSON.stringify({ filename }) });
}

export async function gotoPly(ply) {
  return req('/goto', { method: 'POST', body: JSON.stringify({ ply }) });
}

export async function forward(n = 1) {
  return req('/forward', { method: 'POST', body: JSON.stringify({ n }) });
}

export async function back(n = 1) {
  return req('/back', { method: 'POST', body: JSON.stringify({ n }) });
}

export async function getMoves(file, rank) {
  const q =
    file != null && rank != null
      ? `?file=${file}&rank=${rank}`
      : '';
  return req(`/moves${q}`);
}

export async function applyMove(body) {
  return req('/move', { method: 'POST', body: JSON.stringify(body) });
}

export async function suggest(agent = 'mi') {
  return req('/suggest', { method: 'POST', body: JSON.stringify({ agent }) });
}

export async function playAgent(agent = 'mi') {
  return req('/play', { method: 'POST', body: JSON.stringify({ agent }) });
}

export async function saveGame(filename) {
  return req('/save', {
    method: 'POST',
    body: JSON.stringify({ filename: filename || null }),
  });
}
