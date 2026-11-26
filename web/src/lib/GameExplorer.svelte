<script>
  import { explorerEntries, gameName, parentFolder } from './gameExplorer.js';

  let { games = [], selectedGame = $bindable(''), onLoad, onRefresh } = $props();
  let folder = $state('');
  let query = $state('');
  let page = $state(0);
  const pageSize = 80;
  let refreshing = $state(false);
  let entries = $derived(explorerEntries(games, folder, query));
  let lastPage = $derived(Math.max(0, Math.ceil(entries.length / pageSize) - 1));
  let currentPage = $derived(Math.min(page, lastPage));
  let visible = $derived(entries.slice(currentPage * pageSize, (currentPage + 1) * pageSize));
  let crumbs = $derived(folder ? folder.split('/') : []);

  function navigate(path) {
    folder = path;
    query = '';
    page = 0;
  }

  async function refresh() {
    refreshing = true;
    try {
      await onRefresh();
    } finally {
      refreshing = false;
    }
  }
</script>

<section class="panel games-panel" aria-label="Game explorer">
  <div class="explorer-heading">
    <div><span class="explorer-eyebrow">LIBRARY</span><h3>Game explorer</h3></div>
    <button class="explorer-icon" type="button" onclick={refresh} disabled={refreshing}
      aria-label="Refresh games" title="Refresh games">{refreshing ? '…' : '↻'}</button>
  </div>
  <div class="explorer-search">
    <span aria-hidden="true">⌕</span>
    <input type="search" aria-label="Search all games" placeholder="Search all games…"
      bind:value={query} oninput={() => { page = 0; }} />
  </div>
  <nav class="explorer-breadcrumbs" aria-label="Game folders">
    <button type="button" onclick={() => navigate('')} aria-current={!folder && !query.trim() ? 'location' : undefined}>Library</button>
    {#each crumbs as crumb, i}
      <span aria-hidden="true">/</span>
      <button type="button" title={crumbs.slice(0, i + 1).join('/')}
        aria-current={i === crumbs.length - 1 && !query.trim() ? 'location' : undefined}
        onclick={() => navigate(crumbs.slice(0, i + 1).join('/'))}>{crumb}</button>
    {/each}
  </nav>
  <div class="explorer-caption">
    <span>{query.trim() ? 'Search results · all folders' : 'Name'}</span>
    <span>{entries.length} {query.trim() ? 'games' : 'items'}</span>
  </div>
  <div class="explorer-files" aria-label="Folders and games" tabindex="0">
    {#if folder && !query.trim()}
      <button class="explorer-entry explorer-parent" type="button" onclick={() => navigate(parentFolder(folder))}>
        <span class="explorer-glyph" aria-hidden="true">↑</span><span>Parent folder</span>
      </button>
    {/if}
    {#each visible as entry (entry.path)}
      <button type="button" class="explorer-entry" class:selected={entry.kind === 'file' && selectedGame === entry.path}
        aria-pressed={entry.kind === 'file' ? selectedGame === entry.path : undefined}
        title={entry.path}
        onclick={() => entry.kind === 'folder' ? navigate(entry.path) : selectedGame = entry.path}
        ondblclick={() => { if (entry.kind === 'file') { selectedGame = entry.path; onLoad(); } }}>
        <span class="explorer-glyph" class:folder={entry.kind === 'folder'} aria-hidden="true">{entry.kind === 'folder' ? '▰' : '▤'}</span>
        <span class="explorer-entry-text"><span class="explorer-name">{entry.name}</span>
          {#if query.trim()}<span class="explorer-path">{entry.parent || 'Library'}</span>{/if}
        </span>
        <span class="explorer-entry-meta">{entry.kind === 'folder' ? entry.count : 'JSON'}</span>
        {#if entry.kind === 'folder'}<span aria-hidden="true">›</span>{/if}
      </button>
    {:else}
      <div class="explorer-empty">
        <strong>{query.trim() ? 'No matching games' : games.length ? 'This folder is empty' : 'Your library is empty'}</strong>
        <span>{query.trim() ? 'Try a different name or folder path.' : 'Refresh after adding or saving games.'}</span>
        {#if query.trim() || folder}<button type="button" onclick={() => navigate('')}>Back to library</button>{/if}
      </div>
    {/each}
  </div>
  {#if lastPage > 0}
    <div class="explorer-pagination">
      <button type="button" disabled={currentPage === 0} onclick={() => { page = currentPage - 1; }}>Previous</button>
      <span>{currentPage + 1} / {lastPage + 1}</span>
      <button type="button" disabled={currentPage === lastPage} onclick={() => { page = currentPage + 1; }}>Next</button>
    </div>
  {/if}
  <div class="explorer-footer">
    <div class="explorer-selection" title={selectedGame}>
      <span class="explorer-eyebrow">{selectedGame ? 'SELECTED GAME' : 'NO GAME SELECTED'}</span>
      <strong>{selectedGame ? gameName(selectedGame) : 'Choose a game to review'}</strong>
      {#if selectedGame}<span class="explorer-path">{parentFolder(selectedGame) || 'Library'}</span>{/if}
    </div>
    <button class="explorer-open" type="button" onclick={onLoad} disabled={!selectedGame}>Open game <span aria-hidden="true">↗</span></button>
  </div>
  <p class="explorer-help">Select a game and open it, or double-click its name.</p>
</section>

<style>
  .games-panel { padding: 0; overflow: hidden; border-radius: 12px; background: #fcfbf7; border-color: #d8d4c9; box-shadow: 0 3px 12px #342f2310; min-width: 0; }
  .explorer-heading { display: flex; justify-content: space-between; align-items: center; padding: 16px 16px 12px; }
  .explorer-eyebrow { display: block; font-size: 9px; font-weight: 650; letter-spacing: 0.12em; color: #706d62; margin-bottom: 4px; }
  .explorer-heading h3 { margin: 0; font-size: 16px; font-weight: 600; letter-spacing: -0.025em; }
  button { border: 1px solid transparent; color: inherit; background: transparent; border-radius: 6px; }
  button:focus-visible, input:focus-visible, .explorer-files:focus-visible { outline: 2px solid #2f6b59; outline-offset: -2px; }
  button:disabled { opacity: 0.45; cursor: default; }
  .explorer-icon { width: 32px; height: 32px; border-color: #dedbd1; font-size: 20px; }
  .explorer-icon:hover:enabled { background: #efeee7; }
  .explorer-search { display: flex; align-items: center; gap: 8px; margin: 0 14px 10px; padding: 0 10px; background: #f0efe9; border: 1px solid #dedbd1; border-radius: 7px; color: #757166; }
  .explorer-search > span { font-size: 22px; }
  .explorer-search input { width: 100%; min-width: 0; padding: 9px 0; font-size: 12px; color: #302f29; background: transparent; border: 0; }
  .explorer-search:focus-within { outline: 2px solid #2f6b59; outline-offset: 1px; }
  .explorer-search input:focus-visible { outline: none; }
  .explorer-breadcrumbs { display: flex; align-items: center; gap: 3px; padding: 0 12px 10px; overflow-x: auto; font-size: 11px; color: #736f64; }
  .explorer-breadcrumbs button { padding: 4px 5px; white-space: nowrap; }
  .explorer-breadcrumbs button:hover { background: #efeee7; }
  .explorer-breadcrumbs [aria-current] { color: #285b4b; font-weight: 650; }
  .explorer-caption { display: flex; justify-content: space-between; padding: 8px 16px; font-size: 10px; color: #78756a; border-block: 1px solid #e5e2d9; background: #f6f5ef; }
  .explorer-files { height: 300px; overflow: auto; padding: 5px; scrollbar-width: thin; scrollbar-color: #c4c2b7 transparent; }
  .explorer-entry { display: flex; align-items: center; width: 100%; gap: 9px; padding: 10px 9px; text-align: left; min-height: 40px; font-size: 12px; }
  .explorer-entry:hover { background: #f0efe8; }
  .explorer-entry.selected { background: #e5efe8; border-color: #b9d1c2; color: #234f40; }
  .explorer-entry-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .explorer-name { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .explorer-glyph { flex: 0 0 17px; color: #8a938b; font-size: 19px; text-align: center; }
  .explorer-glyph.folder { color: #b18a45; }
  .explorer-entry-meta { font-size: 9px; color: #817d71; }
  .explorer-parent { color: #777368; font-size: 11px; }
  .explorer-path { font-size: 10px; color: #77776b; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .explorer-empty { display: flex; align-items: center; justify-content: center; text-align: center; flex-direction: column; gap: 8px; padding: 35px 20px; height: 100%; font-size: 12px; color: #777368; }
  .explorer-empty strong { color: #403f35; font-size: 13px; }
  .explorer-empty button { padding: 7px 10px; border-color: #d8d4c9; }
  .explorer-pagination { display: flex; align-items: center; justify-content: space-between; padding: 5px 12px; border-top: 1px solid #e5e2d9; font-size: 11px; color: #6d6a5f; }
  .explorer-pagination button { padding: 6px; }
  .explorer-pagination button:hover:enabled { background: #efeee7; }
  .explorer-footer { padding: 12px 14px; border-top: 1px solid #e5e2d9; }
  .explorer-selection { display: flex; flex-direction: column; gap: 3px; min-width: 0; margin-bottom: 10px; }
  .explorer-selection .explorer-eyebrow { margin: 0; }
  .explorer-selection strong { font-size: 12px; font-weight: 550; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .explorer-open { display: flex; align-items: center; justify-content: space-between; width: 100%; padding: 9px 12px; color: white; background: #315f50; font-size: 12px; font-weight: 550; }
  .explorer-open:hover:enabled { background: #254c3e; }
  .explorer-help { margin: 0; padding: 0 14px 12px; font-size: 10px; color: #817c70; line-height: 1.5; }
  @media (max-width: 900px) { .explorer-files { height: 240px; } }
</style>
