<script>
  let {
    title = 'Search',
    search = null,
    expanded = $bindable(false),
  } = $props();

  function scoreClass(s) {
    if (s == null) return '';
    if (s > 50) return 'pos';
    if (s < -50) return 'neg';
    return 'neu';
  }
</script>

<div class="panel search-panel" class:expanded class:empty={!search}>
  <div class="search-head">
    <h3>{title}{#if search} · {search.agent}{/if}</h3>
    {#if search}
      <button type="button" class="focus-btn" onclick={() => (expanded = !expanded)}>
        {expanded ? 'Collapse' : 'Focus tree'}
      </button>
    {/if}
  </div>
  {#if search}
    <div class="eval-row">
      <span>static <strong class={scoreClass(search.static_eval)}>{search.static_eval}</strong></span>
      <span>→</span>
      <span>search <strong class={scoreClass(search.score)}>{search.score}</strong></span>
      <span class="meta">d{search.depth} · {search.nodes} nodes</span>
    </div>
    {#if search.best_move}
      <div class="best">best: {search.best_move}</div>
    {/if}

    <div class="tree-wrap" class:expanded>
      <ul class="tree">
        {#each search.tree?.children || [] as node}
          <li class:best={node.best} class:cutoff={node.cutoff}>
            <div class="node-line">
              <span class="label">{node.label}</span>
              <span class={scoreClass(node.score)}>{node.score ?? '—'}</span>
              {#if node.best}<span class="tag">best</span>{/if}
              {#if node.cutoff}<span class="tag cut">cut</span>{/if}
            </div>
            {#if node.children?.length}
              <ul>
                {#each node.children as child}
                  <li class:best={child.best} class:cutoff={child.cutoff}>
                    <div class="node-line">
                      <span class="label">{child.label}</span>
                      <span class={scoreClass(child.score)}>{child.score ?? '—'}</span>
                      {#if child.best}<span class="tag">best</span>{/if}
                      {#if child.cutoff}<span class="tag cut">cut</span>{/if}
                    </div>
                  </li>
                {/each}
              </ul>
            {/if}
          </li>
        {/each}
      </ul>
    </div>
  {:else}
    <p class="hint">No search yet</p>
  {/if}
</div>

<style>
  .search-panel {
    font-size: 0.8rem;
  }
  .search-panel.empty {
    opacity: 0.75;
  }
  .search-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .search-head h3 {
    margin: 0;
  }
  .focus-btn {
    font-size: 0.75rem;
    padding: 0.15rem 0.45rem;
  }
  .eval-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    align-items: baseline;
    margin: 0.35rem 0;
  }
  .meta {
    color: #6a6356;
  }
  .best {
    margin-bottom: 0.35rem;
    font-family: "IBM Plex Mono", ui-monospace, monospace;
  }
  .hint {
    margin: 0.25rem 0 0;
    color: #6a6356;
    font-size: 0.75rem;
  }
  .tree-wrap {
    max-height: 7rem;
    overflow: auto;
    border: 1px solid #d4cdb8;
    background: #fffdf7;
    padding: 0.25rem 0.4rem;
  }
  .tree-wrap.expanded {
    max-height: min(50vh, 24rem);
  }
  .search-panel.expanded {
    position: relative;
    z-index: 2;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.12);
  }
  .tree {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .tree ul {
    list-style: none;
    margin: 0.15rem 0 0.25rem 0.75rem;
    padding: 0;
    border-left: 1px solid #c9c2b0;
  }
  .node-line {
    display: flex;
    gap: 0.4rem;
    align-items: baseline;
    font-family: "IBM Plex Mono", ui-monospace, monospace;
    font-size: 0.72rem;
    padding: 0.1rem 0;
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  li.best > .node-line {
    font-weight: 600;
  }
  li.cutoff > .node-line {
    opacity: 0.75;
  }
  .tag {
    font-size: 0.65rem;
    text-transform: uppercase;
    color: #2f5d50;
  }
  .tag.cut {
    color: #a15c2e;
  }
  .pos {
    color: #1a6b3c;
  }
  .neg {
    color: #9b2c2c;
  }
  .neu {
    color: #444;
  }
</style>
