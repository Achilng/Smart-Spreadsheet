<script lang="ts">
  import type { TagMatchMode } from "../../api";
  import { errorText, formatCount } from "../app-state.svelte";
  import { rowStore, setFilter } from "../row-store.svelte";
  import { clearSelection, selectAllFiltered } from "../selection-store.svelte";
  import { createTagAndRefresh, tagStore } from "../tag-store.svelte";

  let newTagName = $state("");
  let creating = $state(false);
  let status = $state<{ text: string; isError: boolean } | null>(null);

  const activeTags = $derived(rowStore.tags);

  /** Tag 库列表 + 仍在筛选中但已被自动清理的 Tag（计数 0） */
  const entries = $derived.by(() => {
    const known = new Set(tagStore.list.map(tag => tag.name));
    const merged = tagStore.list.map(tag => ({ name: tag.name, rowCount: tag.rowCount }));
    for (const active of activeTags) {
      if (!known.has(active)) {
        merged.push({ name: active, rowCount: 0 });
      }
    }
    return merged;
  });

  function toggleFilterTag(name: string): void {
    const next = activeTags.includes(name)
      ? activeTags.filter(tag => tag !== name)
      : [...activeTags, name];
    setFilter(next, rowStore.tagMode);
    clearSelection();
  }

  function setMode(mode: TagMatchMode): void {
    if (mode !== rowStore.tagMode) {
      setFilter(activeTags, mode);
      clearSelection();
    }
  }

  function clearFilter(): void {
    if (activeTags.length > 0) {
      setFilter([], rowStore.tagMode);
      clearSelection();
    }
  }

  async function createNew(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const name = newTagName.trim();
    if (!name) {
      status = { text: "请输入一个非空 Tag。", isError: true };
      return;
    }
    creating = true;
    status = null;
    try {
      const created = await createTagAndRefresh(name);
      status = created
        ? { text: `已新建 Tag“${name}”。`, isError: false }
        : { text: `Tag“${name}”已存在。`, isError: false };
      newTagName = "";
    } catch (error) {
      status = { text: `新建失败：${errorText(error)}`, isError: true };
    } finally {
      creating = false;
    }
  }
</script>

<div class="tag-sidebar">
  <header class="sidebar-header">
    <h3>Tag 库</h3>
    <div class="mode-switch" role="group" aria-label="筛选模式">
      <button
        type="button"
        class:is-active={rowStore.tagMode === "and"}
        aria-pressed={rowStore.tagMode === "and"}
        onclick={() => setMode("and")}
      >
        AND
      </button>
      <button
        type="button"
        class:is-active={rowStore.tagMode === "or"}
        aria-pressed={rowStore.tagMode === "or"}
        onclick={() => setMode("or")}
      >
        OR
      </button>
    </div>
  </header>

  <div class="filter-info">
    <span class="faint">{formatCount(rowStore.totalCount)} 条匹配</span>
    <span class="filter-links">
      {#if rowStore.totalCount > 0}
        <button type="button" class="link-btn" onclick={() => void selectAllFiltered()}>全选</button>
      {/if}
      {#if activeTags.length > 0}
        <button type="button" class="link-btn" onclick={clearFilter}>清除筛选</button>
      {/if}
    </span>
  </div>

  <div class="tag-list">
    {#if tagStore.error}
      <p class="list-note">Tag 列表加载失败：{tagStore.error}</p>
    {:else if entries.length === 0}
      <p class="list-note faint">还没有 Tag。在下方新建后，点击即可筛选。</p>
    {:else}
      {#each entries as entry (entry.name)}
        <button
          type="button"
          class="tag-row"
          class:is-active={activeTags.includes(entry.name)}
          aria-pressed={activeTags.includes(entry.name)}
          onclick={() => toggleFilterTag(entry.name)}
        >
          <span class="tag-name" title={entry.name}>{entry.name}</span>
          <span class="tag-count">{formatCount(entry.rowCount)}</span>
        </button>
      {/each}
    {/if}
  </div>

  <form class="create-form" onsubmit={createNew}>
    <input
      type="text"
      placeholder="新建 Tag（区分大小写）"
      bind:value={newTagName}
      disabled={creating}
      autocomplete="off"
    />
    <button type="submit" class="btn" disabled={creating}>新建</button>
  </form>
  {#if status}
    <p class="form-status" class:is-error={status.isError} role="status">{status.text}</p>
  {/if}
</div>

<style>
  .tag-sidebar {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    flex: none;
  }

  .sidebar-header h3 {
    font-size: 13px;
    font-weight: 600;
  }

  .mode-switch {
    display: flex;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    padding: 2px;
    gap: 2px;
  }

  .mode-switch button {
    border: none;
    background: transparent;
    border-radius: 4px;
    padding: 1px 8px;
    font-size: 11px;
    color: var(--text-2);
  }

  .mode-switch button.is-active {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow-1);
  }

  .filter-info {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px 8px;
    font-size: 12px;
    flex: none;
  }

  .filter-links {
    display: flex;
    gap: 10px;
  }

  .link-btn {
    border: none;
    background: none;
    color: var(--accent);
    font-size: 12px;
    padding: 0;
  }

  .link-btn:hover {
    text-decoration: underline;
  }

  .tag-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .list-note {
    padding: 8px 6px;
    font-size: 12px;
  }

  .tag-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    border: none;
    background: none;
    border-radius: var(--radius-s);
    padding: 5px 8px;
    font-size: 13px;
    text-align: left;
    color: var(--text);
  }

  .tag-row:hover {
    background: var(--surface-2);
  }

  .tag-row.is-active {
    background: var(--accent-soft);
    color: var(--accent);
  }

  .tag-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag-count {
    font-size: 11px;
    color: var(--text-3);
    flex: none;
  }

  .tag-row.is-active .tag-count {
    color: var(--accent);
  }

  .create-form {
    display: flex;
    gap: 6px;
    padding: 10px 12px 4px;
    border-top: 1px solid var(--border);
    flex: none;
  }

  .create-form input {
    flex: 1;
    min-width: 0;
    padding: 5px 9px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    font-size: 12.5px;
  }

  .create-form input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .create-form .btn {
    padding: 4px 10px;
    font-size: 12.5px;
  }

  .form-status {
    padding: 2px 12px 8px;
    font-size: 11.5px;
    color: var(--success);
  }

  .form-status.is-error {
    color: var(--danger);
  }
</style>
