<script lang="ts">
  import X from "@lucide/svelte/icons/x";

  import { app, formatCount } from "../../stores/app-state.svelte";
  import { duplicateBrowse } from "../../stores/duplicate-browse-store.svelte";
  import { groupBrowse } from "../../stores/group-browse-store.svelte";
  import { groupStore } from "../../stores/group-store.svelte";
  import {
    clearAllFilters,
    rowStore,
    removeLibraryFilter,
    setArtistFilter,
    setDedupe,
    setFilter,
    setHasVibe,
    setHideGrouped,
    setSearch,
    setSingleArtistOnly,
    setUntaggedOnly,
  } from "../../stores/row-store.svelte";
  import { clearSelection, resetSelectionAnchor } from "../../stores/selection-store.svelte";
  import { softFade, softPop } from "../../ui/motion";
  import type { DedupeMode } from "../../api";
  import SizeSlider from "./SizeSlider.svelte";
  import SortControl from "./SortControl.svelte";
  import { viewLabel } from "./view-modes";
  import { libraryFilterLabel } from "../../utils/library-filters";

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
    if (duplicateBrowse.dedupeMode !== mode) {
      duplicateBrowse.dedupeMode = mode;
      clearSelection();
    }
  }

  function setGroupSort(sortByCount: boolean): void {
    if (groupBrowse.sortByCount !== sortByCount) {
      groupBrowse.sortByCount = sortByCount;
      resetSelectionAnchor();
    }
  }

  function setDuplicateSort(sortByCount: boolean): void {
    if (duplicateBrowse.sortByCount !== sortByCount) {
      duplicateBrowse.sortByCount = sortByCount;
      resetSelectionAnchor();
    }
  }

  const searchTerm = $derived(rowStore.search.trim());
  const hasAnyFilter = $derived(
    searchTerm !== "" ||
      rowStore.tags.length > 0 ||
      rowStore.dedupe !== "none" ||
      rowStore.singleArtistOnly ||
      rowStore.artistFilter !== "" ||
      rowStore.hasVibe ||
      rowStore.untaggedOnly ||
      rowStore.filters.length > 0 ||
      rowStore.hideGrouped,
  );

  function removeTag(tag: string): void {
    setFilter(rowStore.tags.filter(t => t !== tag), rowStore.tagMode);
    clearSelection();
  }

  function clearAll(): void {
    clearAllFilters();
    clearSelection();
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
          onclick={() => setGroupSort(false)}
        >按名称</button>
        <button
          type="button"
          class="txt-opt"
          class:is-active={groupBrowse.sortByCount}
          onclick={() => setGroupSort(true)}
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
        <button
          type="button"
          class="txt-opt"
          class:is-active={duplicateBrowse.dedupeMode === "vibes"}
          onclick={() => setDedupeMode("vibes")}
        >按 VIBE</button>
      </div>
      <span class="txt-sep" aria-hidden="true"></span>
      <div class="txt-group" role="group" aria-label="重复排序">
        <button
          type="button"
          class="txt-opt"
          class:is-active={duplicateBrowse.sortByCount}
          onclick={() => setDuplicateSort(true)}
        >按数量</button>
        <button
          type="button"
          class="txt-opt"
          class:is-active={!duplicateBrowse.sortByCount}
          onclick={() => setDuplicateSort(false)}
        >按名称</button>
      </div>
    {/if}
  </div>
</div>

{#if hasAnyFilter}
  <div class="chips">
    {#if searchTerm !== ""}
      <span class="chip-f" transition:softPop={{ duration: 150, y: 0, start: 0.92 }}>
        <span class="chip-text">“{searchTerm}”</span>
        <button type="button" class="x" title="清除搜索" onclick={() => { setSearch(""); clearSelection(); }}>
          <X size={11} strokeWidth={2.2} />
        </button>
      </span>
    {/if}
    {#each rowStore.tags as tag (tag)}
      <span class="chip-f" transition:softPop={{ duration: 150, y: 0, start: 0.92 }}>
        <b class="chip-text">{tag}</b>
        <button type="button" class="x" title="移除 Tag 筛选" onclick={() => removeTag(tag)}>
          <X size={11} strokeWidth={2.2} />
        </button>
      </span>
    {/each}
    {#if rowStore.dedupe === "positivePrompt"}
      <span class="chip-f" transition:softPop={{ duration: 150, y: 0, start: 0.92 }}>按正向去重
        <button type="button" class="x" title="取消去重" onclick={() => { setDedupe("none"); clearSelection(); }}>
          <X size={11} strokeWidth={2.2} />
        </button>
      </span>
    {:else if rowStore.dedupe === "artists"}
      <span class="chip-f" transition:softPop={{ duration: 150, y: 0, start: 0.92 }}>按画师串去重
        <button type="button" class="x" title="取消去重" onclick={() => { setDedupe("none"); clearSelection(); }}>
          <X size={11} strokeWidth={2.2} />
        </button>
      </span>
    {/if}
    {#if rowStore.singleArtistOnly}
      <span class="chip-f" transition:softPop={{ duration: 150, y: 0, start: 0.92 }}>单画师串
        <button type="button" class="x" title="取消筛选" onclick={() => { setSingleArtistOnly(false); clearSelection(); }}>
          <X size={11} strokeWidth={2.2} />
        </button>
      </span>
    {/if}
    {#if rowStore.artistFilter !== ""}
      <span class="chip-f" transition:softPop={{ duration: 150, y: 0, start: 0.92 }} title={rowStore.artistFilter}>
        <span class="chip-text">画师串：{rowStore.artistFilter}</span>
        <button type="button" class="x" title="取消画师串筛选" onclick={() => { setArtistFilter(""); clearSelection(); }}>
          <X size={11} strokeWidth={2.2} />
        </button>
      </span>
    {/if}
    {#if rowStore.hasVibe}
      <span class="chip-f" transition:softPop={{ duration: 150, y: 0, start: 0.92 }}>VIBE
        <button type="button" class="x" title="取消筛选" onclick={() => { setHasVibe(false); clearSelection(); }}>
          <X size={11} strokeWidth={2.2} />
        </button>
      </span>
    {/if}
    {#if rowStore.untaggedOnly}
      <span class="chip-f" transition:softPop={{ duration: 150, y: 0, start: 0.92 }}>无 Tag
        <button type="button" class="x" title="取消筛选" onclick={() => { setUntaggedOnly(false); clearSelection(); }}>
          <X size={11} strokeWidth={2.2} />
        </button>
      </span>
    {/if}
    {#if rowStore.hideGrouped}
      <span class="chip-f" transition:softPop={{ duration: 150, y: 0, start: 0.92 }}>隐藏已分组
        <button type="button" class="x" title="取消筛选" onclick={() => { setHideGrouped(false); clearSelection(); }}>
          <X size={11} strokeWidth={2.2} />
        </button>
      </span>
    {/if}
    {#each rowStore.filters as filter, index (`${filter.type}-${index}`)}
      <span class="chip-f" transition:softPop={{ duration: 150, y: 0, start: 0.92 }} title={libraryFilterLabel(filter, groupStore.list)}>
        <span class="chip-text">{libraryFilterLabel(filter, groupStore.list)}</span>
        <button type="button" class="x" title="移除此过滤条件" onclick={() => { removeLibraryFilter(index); clearSelection(); }}>
          <X size={11} strokeWidth={2.2} />
        </button>
      </span>
    {/each}
    <button type="button" class="chip-clear" transition:softFade={{ duration: 130 }} onclick={clearAll}>清除全部</button>
  </div>
{/if}

<style>
  .canvas-head {
    flex: none;
    display: flex;
    align-items: baseline;
    gap: 12px;
    padding: 22px 26px 0;
    position: relative;
    z-index: var(--z-dropdown);
    flex-wrap: wrap;
    row-gap: 6px;
  }

  @media (max-width: 1080px) {
    .canvas-head {
      padding: 16px 18px 0;
    }
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
    flex-wrap: wrap;
    min-width: 0;
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

  /* ---- 筛选 chips 行 ---- */
  .chips {
    flex: none;
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
    padding: 12px 26px 0;
    align-items: center;
    position: relative;
    z-index: 1;
  }

  @media (max-width: 1080px) {
    .chips {
      padding: 10px 18px 0;
    }
  }

  .chip-f {
    height: 26px;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 0 6px 0 11px;
    border-radius: var(--radius-full);
    font-size: var(--font-sm);
    background: var(--surface);
    border: 1px solid var(--border-strong);
    color: var(--text);
    white-space: nowrap;
  }

  .chip-f b,
  .chip-f .chip-text {
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chip-f b {
    font-weight: 600;
  }

  .chip-f .x {
    width: 16px;
    height: 16px;
    flex: none;
    border-radius: 50%;
    border: none;
    background: none;
    display: grid;
    place-items: center;
    color: var(--text-3);
    padding: 0;
    cursor: pointer;
  }

  .chip-f .x:hover {
    background: var(--surface-3);
    color: var(--text);
  }

  .chip-clear {
    border: none;
    background: none;
    font-size: var(--font-sm);
    color: var(--accent);
    padding: 2px 4px;
    margin-left: 4px;
  }

  .chip-clear:hover {
    text-decoration: underline;
  }
</style>
