<script lang="ts">
  import type { SortMode } from "../../api";
  import { rowStore, setSort } from "../../stores/row-store.svelte";

  let { controlId }: { controlId: string } = $props();

  function onChange(event: Event): void {
    setSort((event.currentTarget as HTMLSelectElement).value as SortMode);
  }
</script>

<div class="sort-toolbar">
  <label for={controlId}>排序方式</label>
  <select id={controlId} value={rowStore.sort} onchange={onChange}>
    <option value="timeAsc">时间正序</option>
    <option value="timeDesc">时间倒序</option>
    <option value="recentlyUpdated">最近更新</option>
  </select>
</div>

<style>
  .sort-toolbar {
    height: 42px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding: 0 16px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }

  label {
    color: var(--text-2);
    font-size: var(--font-sm);
    font-weight: 600;
  }

  select {
    min-width: 128px;
    height: 28px;
    padding: 0 28px 0 9px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    background: var(--surface-2);
    color: var(--text);
    font: inherit;
    font-size: var(--font-sm);
    cursor: pointer;
  }

  select:hover {
    border-color: var(--text-3);
  }

  select:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>
