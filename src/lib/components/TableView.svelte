<script lang="ts">
  import type { RowRecord } from "../../api";
  import { app } from "../app-state.svelte";
  import { PAGE_SIZE, ensurePage, getRow, resetRows, rowStore } from "../row-store.svelte";
  import { thumbnails } from "../thumbnails";
  import { saveScrollPosition, savedScrollPosition } from "../view-state";
  import TableRow from "./TableRow.svelte";

  const HEADER_HEIGHT = 36;
  const OVERSCAN = 6;

  const rowHeight = $derived(app.tableRowHeight);
  const thumbColWidth = $derived(Math.max(52, Math.round(rowHeight * 1.15)));
  const gridCols = $derived(
    `36px ${thumbColWidth}px 64px 150px minmax(0, 2.2fr) minmax(0, 1.2fr) minmax(0, 1.4fr)`,
  );

  let viewport = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewportHeight = $state(0);

  const spacerHeight = $derived(rowStore.totalCount * rowHeight);

  interface Item {
    index: number;
    row: RowRecord | undefined;
    y: number;
  }

  const items = $derived.by(() => {
    void rowStore.pagesVersion;
    if (rowStore.totalCount === 0 || viewportHeight <= 0) {
      return [] as Item[];
    }
    const first = Math.max(0, Math.floor((scrollTop - HEADER_HEIGHT) / rowHeight) - OVERSCAN);
    const visibleCount = Math.ceil(viewportHeight / rowHeight) + OVERSCAN * 2;
    const last = Math.min(rowStore.totalCount, first + visibleCount);
    const result: Item[] = [];
    for (let index = first; index < last; index += 1) {
      result.push({ index, row: getRow(index), y: index * rowHeight });
    }
    return result;
  });

  // 可见区域变化时补加载缺页，并丢弃滚出视野的缩略图请求
  $effect(() => {
    const missingPages = new Set<number>();
    const visibleRowIds = new Set<number>();
    for (const item of items) {
      if (item.row) {
        visibleRowIds.add(item.row.id);
      } else {
        missingPages.add(Math.floor(item.index / PAGE_SIZE));
      }
    }
    thumbnails.retain(visibleRowIds);
    for (const pageIndex of missingPages) {
      ensurePage(pageIndex);
    }
  });

  // 挂载后恢复上次离开时的滚动位置。spacer 高度依赖 totalCount，必须等
  // 数据就绪；且视图切换可能触发换代刷新（如分组视图的行集合语义不同），
  // 刷新在途时 totalCount 还是旧语义的值，提前恢复会被钳制到错误位置。
  let restored = false;
  $effect(() => {
    if (restored || !viewport || rowStore.initialLoading || rowStore.refreshing) {
      return;
    }
    void rowStore.totalCount;
    restored = true;
    const saved = savedScrollPosition("table");
    if (saved > 0) {
      viewport.scrollTop = saved;
      scrollTop = viewport.scrollTop;
    }
  });

  // 筛选/搜索/数据变更导致结果集语义变化时回到顶部
  let seenReset = rowStore.resetToken;
  $effect(() => {
    if (rowStore.resetToken !== seenReset) {
      seenReset = rowStore.resetToken;
      scrollTop = 0;
      if (viewport) {
        viewport.scrollTop = 0;
      }
    }
  });

  function onScroll(): void {
    scrollTop = viewport?.scrollTop ?? 0;
    saveScrollPosition("table", scrollTop);
  }
</script>

<div
  class="table-viewport"
  bind:this={viewport}
  bind:clientHeight={viewportHeight}
  onscroll={onScroll}
>
  {#if rowStore.error}
    <div class="table-status">
      <p class="muted">加载失败：{rowStore.error}</p>
      <button type="button" class="btn" onclick={() => resetRows()}>重试</button>
    </div>
  {:else if rowStore.initialLoading}
    <div class="table-status">
      <p class="muted">正在加载…</p>
    </div>
  {:else if rowStore.totalCount === 0}
    <div class="table-status">
      <p class="muted">
        {rowStore.tags.length > 0 ? "当前筛选没有匹配记录。" : "工作簿没有数据行。"}
      </p>
    </div>
  {:else}
    <div class="table-header" style:height="{HEADER_HEIGHT}px" style:grid-template-columns={gridCols} role="row">
      <span></span>
      <span>图片</span>
      <span>行号</span>
      <span>时间</span>
      <span>正向提示词</span>
      <span>画师串</span>
      <span>Tags</span>
    </div>
    <div class="table-spacer" style:height="{spacerHeight}px" role="rowgroup">
      {#each items as item (item.index)}
        <TableRow row={item.row} index={item.index} y={item.y} height={rowHeight} {gridCols} {thumbColWidth} />
      {/each}
    </div>
  {/if}
</div>

<style>
  .table-viewport {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    position: relative;
    background: var(--surface);
  }

  .table-header {
    position: sticky;
    top: 0;
    z-index: 10;
    display: grid;
    align-items: center;
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    color: var(--text-2);
    font-weight: 600;
  }

  .table-header span {
    padding: 0 10px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .table-spacer {
    position: relative;
  }

  .table-status {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
  }
</style>
