export function pathParts(path) {
  return String(path).replace(/\\/g, '/').split('/').filter(Boolean);
}

export function gameName(path) {
  return pathParts(path).at(-1) || path;
}

export function parentFolder(path) {
  return pathParts(path).slice(0, -1).join('/');
}

// Preserve the original API path for loading; normalized paths are for browsing.
export function explorerEntries(games, folder = '', query = '') {
  const needle = query.trim().toLowerCase();
  const folders = new Map();
  const files = [];
  const prefix = folder ? `${folder}/` : '';
  for (const path of games) {
    const normalized = pathParts(path).join('/');
    if (needle) {
      if (normalized.toLowerCase().includes(needle)) {
        files.push({ kind: 'file', path, name: gameName(path), parent: parentFolder(path) });
      }
      continue;
    }
    if (!normalized.startsWith(prefix)) continue;
    const relative = normalized.slice(prefix.length);
    const slash = relative.indexOf('/');
    if (slash >= 0) {
      const name = relative.slice(0, slash);
      const entry = folders.get(name) || { kind: 'folder', path: prefix + name, name, count: 0 };
      entry.count += 1;
      folders.set(name, entry);
    } else {
      files.push({ kind: 'file', path, name: relative, parent: folder });
    }
  }
  const compare = (a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }) || a.path.localeCompare(b.path);
  return [...[...folders.values()].sort(compare), ...files.sort(compare)];
}
