<script lang="ts">
  import { app, formatCount, type ViewMode } from "../../stores/app-state.svelte";
  import { duplicateBrowse } from "../../stores/duplicate-browse-store.svelte";
  import { groupStore } from "../../stores/group-store.svelte";
  import { clearSelection } from "../../stores/selection-store.svelte";
  import { VIEW_MODES } from "./view-modes";

  function switchView(mode: ViewMode): void {
    const wasGroup = app.viewMode === "group";
    const isGroup = mode === "group";
    app.viewMode = mode;
    if (wasGroup !== isGroup) {
      clearSelection();
    }
  }

  function countFor(mode: ViewMode): number | null {
    if (mode === "group" && groupStore.list.length > 0) {
      return groupStore.list.length;
    }
    if (mode === "duplicates" && duplicateBrowse.clusters.length > 0) {
      return duplicateBrowse.clusters.length;
    }
    return null;
  }

  function ariaLabelFor(mode: ViewMode, label: string): string {
    const count = countFor(mode);
    return count != null ? `${label}，${formatCount(count)} 个` : label;
  }
</script>

<nav class="segmented" aria-label="视图切换">
  {#each VIEW_MODES as view (view.mode)}
    <button
      type="button"
      class="seg"
      class:is-active={app.viewMode === view.mode}
      aria-pressed={app.viewMode === view.mode}
      aria-label={ariaLabelFor(view.mode, view.label)}
      onclick={() => switchView(view.mode)}
    >
      {view.label}
      {#if countFor(view.mode) != null}
        <span class="n" aria-hidden="true">{formatCount(countFor(view.mode) ?? 0)}</span>
      {/if}
    </button>
  {/each}
</nav>

<style>
  .segmented {
    display: flex;
    background: var(--surface-3);
    border-radius: var(--radius-full);
    padding: 3px;
    gap: 2px;
    flex: none;
  }

  .seg {
    height: 28px;
    padding: 0 16px;
    border: none;
    background: transparent;
    border-radius: var(--radius-full);
    display: inline-flex;
    align-items: center;
    font-size: 12.5px;
    color: var(--text-2);
    white-space: nowrap;
    transition:
      background var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive),
      box-shadow var(--motion-fast) var(--ease-responsive);
  }

  .seg:hover:not(.is-active) {
    color: var(--text);
  }

  .seg.is-active {
    background: var(--surface);
    color: var(--text);
    font-weight: 600;
    box-shadow: 0 1px 4px rgb(0 0 0 / 10%);
  }

  .seg .n {
    font-size: 10.5px;
    color: var(--text-3);
    margin-left: 6px;
    font-variant-numeric: tabular-nums;
  }
</style>
