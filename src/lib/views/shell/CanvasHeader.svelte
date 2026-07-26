<script lang="ts">
  import { app, formatCount } from "../../stores/app-state.svelte";
  import { duplicateBrowse } from "../../stores/duplicate-browse-store.svelte";
  import { groupBrowse } from "../../stores/group-browse-store.svelte";
  import { groupStore } from "../../stores/group-store.svelte";
  import { rowStore } from "../../stores/row-store.svelte";
  import type { DedupeMode } from "../../api";
  import SizeSlider from "./SizeSlider.svelte";
  import SortControl from "./SortControl.svelte";
  import { viewLabel } from "./view-modes";

  const title = $derived(viewLabel(app.viewMode));

  const countText = $derived.by(() => {
    switch (app.viewMode) {
      case "gallery":
      case "table":
        return `${formatCount(rowStore.totalCount)} 张符合条件`;
      case "group":
        return `${formatCount(groupStore.list.length)} 个分组`;
      case "duplicates":
        return duplicateBrowse.clusters.length > 0
          ? `${formatCount(duplicateBrowse.clusters.length)} 组重复`
          : "";
      default:
        return "";
    }
  });

  function setDedupeMode(mode: DedupeMode): void {
    duplicateBrowse.dedupeMode = mode;
  }
</script>

<div class="canvas-head">
  <span class="c-title">{title}</span>
  {#if countText}
    <span class="c-count">{countText}</span>
  {/if}
  <span class="c-spacer"></span>

  <div class="c-controls">
    {#if app.viewMode === "gallery" || app.viewMode === "table"}
      <SizeSlider />
      <SortControl controlId="canvas-sort" />
    {:else if app.viewMode === "group"}
      <div class="txt-group" role="group" aria-label="分组排序">
        <button
          type="button"
          class="txt-opt"
          class:is-active={!groupBrowse.sortByCount}
          onclick={() => (groupBrowse.sortByCount = false)}
        >按名称</button>
        <button
          type="button"
          class="txt-opt"
          class:is-active={groupBrowse.sortByCount}
          onclick={() => (groupBrowse.sortByCount = true)}
        >按数量</button>
      </div>
      <button
        type="button"
        class="txt-action"
        onclick={() => (app.groupManageOpen = true)}
      >管理分组</button>
    {:else if app.viewMode === "duplicates"}
      <div class="txt-group" role="group" aria-label="聚合依据">
        <button
          type="button"
          class="txt-opt"
          class:is-active={duplicateBrowse.dedupeMode === "artists"}
          onclick={() => setDedupeMode("artists")}
        >按画师串</button>
        <button
          type="button"
          class="txt-opt"
          class:is-active={duplicateBrowse.dedupeMode === "positivePrompt"}
          onclick={() => setDedupeMode("positivePrompt")}
        >按正向提示词</button>
      </div>
      <span class="txt-sep" aria-hidden="true"></span>
      <div class="txt-group" role="group" aria-label="重复排序">
        <button
          type="button"
          class="txt-opt"
          class:is-active={duplicateBrowse.sortByCount}
          onclick={() => (duplicateBrowse.sortByCount = true)}
        >按数量</button>
        <button
          type="button"
          class="txt-opt"
          class:is-active={!duplicateBrowse.sortByCount}
          onclick={() => (duplicateBrowse.sortByCount = false)}
        >按名称</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .canvas-head {
    flex: none;
    display: flex;
    align-items: baseline;
    gap: 12px;
    padding: 22px 26px 0;
    position: relative;
    z-index: var(--z-dropdown);
  }

  .c-title {
    font-size: 24px;
    font-weight: 700;
    letter-spacing: -0.022em;
    color: var(--text);
  }

  .c-count {
    font-size: 12.5px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }

  .c-spacer {
    flex: 1;
  }

  .c-controls {
    display: flex;
    align-items: center;
    gap: 10px;
    align-self: center;
  }

  .txt-group {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .txt-opt {
    border: none;
    background: none;
    padding: 4px 8px;
    border-radius: var(--radius-full);
    font-size: 12.5px;
    color: var(--text-2);
    transition:
      background var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive);
  }

  .txt-opt:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .txt-opt.is-active {
    color: var(--text);
    font-weight: 700;
  }

  .txt-action {
    border: none;
    background: none;
    padding: 4px 8px;
    border-radius: var(--radius-full);
    font-size: 12.5px;
    color: var(--accent);
  }

  .txt-action:hover {
    background: var(--surface-2);
  }

  .txt-sep {
    width: 1px;
    height: 14px;
    background: var(--border-strong);
  }
</style>
