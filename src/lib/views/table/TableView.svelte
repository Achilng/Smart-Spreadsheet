<script lang="ts">
  import { onDestroy } from "svelte";

  import type { RowRecord } from "../../api";
  import { app } from "../../stores/app-state.svelte";
  import { PAGE_SIZE, ensurePage, getRow, resetRows, rowStore } from "../../stores/row-store.svelte";
  import { thumbnails } from "../../images/thumbnails";
  import { restoreScrollPosition, saveScrollPosition, scrollPositionVersion } from "../../stores/view-state";
  import TableRow from "./TableRow.svelte";
  import { softFade } from "../../ui/motion";

  let { active = true }: { active?: boolean } = $props();

  const HEADER_HEIGHT = 36;
  const OVERSCAN = 6;

  const rowHeight = $derived(app.tableRowHeight);
  const thumbColWidth = $derived(Math.max(52, Math.round(rowHeight * 1.15)));
  const gridCols = $derived(
    `36px ${thumbColWidth}px 64px 150px minmax(0, 1.8fr) minmax(0, 1.8fr) minmax(0, 1.1fr) minmax(0, 1.3fr)`,
  );

  let viewport = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let measuredHeight = $state(0);
  // 面板隐藏（display:none）时 ResizeObserver 会报 0，冻结最后一次有效
  // 高度，避免隐藏期间可见区计算退化（与画廊视图同一套保活约定）。
  let viewportHeight = $state(0);
  let lastScrollSaveVersion = scrollPositionVersion();

  $effect(() => {
    if (measuredHeight > 0) {
      viewportHeight = measuredHeight;
    }
  });

  const spacerHeight = $derived(rowStore.totalCount * rowHeight);

  interface Item {
    index: number;
    row: RowRecord | undefined;
    y: number;
  }

  const items = $derived.by(() => {
    void rowStore.pagesVersion;
    if (!active || rowStore.totalCount === 0 || viewportHeight <= 0) {
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
    if (!active || viewportHeight <= 0) {
      return;
    }
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

  // 挂载和每次切回激活时恢复滚动位置。spacer 高度依赖 totalCount，必须等
  // 数据就绪；刷新在途时 totalCount 还是旧语义的值，提前恢复会被钳制。
  // 保活后浏览器通常能自行保留位置，这里作为钳制后的兜底按帧重试恢复。
  let restored = false;
  let restoring = false;
  let pendingReset = false;

  $effect(() => {
    if (!active) {
      restored = false;
    }
  });

  $effect(() => {
    if (
      restored ||
      !active ||
      !viewport ||
      rowStore.initialLoading ||
      rowStore.refreshing ||
      viewportHeight <= 0 ||
      spacerHeight <= 0
    ) {
      return;
    }
    void rowStore.totalCount;
    void spacerHeight;
    restored = true;
    if (pendingReset) {
      // 隐藏期间发生过结果集重置：对 display:none 元素设置 scrollTop 会被
      // 浏览器忽略，只能等到重新显示后在这里执行回顶。
      pendingReset = false;
      scrollTop = 0;
      viewport.scrollTop = 0;
      return;
    }
    restoring = true;
    restoreScrollPosition(
      viewport,
      "table",
      60,
      top => (scrollTop = top),
      () => (restoring = false),
    );
  });

  // 筛选/搜索/数据变更导致结果集语义变化时回到顶部；隐藏期间挂起到激活时执行
  let seenReset = rowStore.resetToken;
  $effect(() => {
    if (rowStore.resetToken !== seenReset) {
      seenReset = rowStore.resetToken;
      if (active && viewport) {
        scrollTop = 0;
        viewport.scrollTop = 0;
      } else {
        pendingReset = true;
      }
    }
  });

  function onScroll(): void {
    scrollTop = viewport?.scrollTop ?? 0;
    // 恢复期间的钳制事件和隐藏状态下的读数不代表用户位置，不能写入记忆
    if (active && !restoring) {
      saveTableScroll(scrollTop);
    }
  }

  function saveTableScroll(top: number): void {
    saveScrollPosition("table", top);
    lastScrollSaveVersion = scrollPositionVersion();
  }

  onDestroy(() => {
    // 隐藏状态下 scrollTop 恒为 0，只有激活状态的读数才可信
    if (active && viewport && lastScrollSaveVersion === scrollPositionVersion()) {
      saveTableScroll(viewport.scrollTop);
    }
  });
</script>

<div class="table-view">
  <div
    class="table-viewport"
    bind:this={viewport}
    bind:clientHeight={measuredHeight}
    onscroll={onScroll}
  >
    {#if rowStore.error}
      <div class="table-status empty-state" transition:softFade={{ duration: 140 }}>
        <p class="muted">加载失败：{rowStore.error}</p>
        <button type="button" class="btn" onclick={() => resetRows()}>重试</button>
      </div>
    {:else if rowStore.initialLoading}
      <div class="table-status empty-state" transition:softFade={{ duration: 140 }}>
        <p class="muted">正在加载…</p>
      </div>
    {:else if rowStore.totalCount === 0}
      <div class="table-status empty-state" transition:softFade={{ duration: 140 }}>
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
        <span>角色提示词</span>
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
</div>

<style>
  .table-view {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

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
    z-index: var(--z-nav);
    display: grid;
    align-items: center;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    font-size: var(--font-sm);
    color: var(--text-2);
    font-weight: 600;
  }

  .table-header span {
    padding: 0 10px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: var(--font-xs);
    font-weight: 650;
    letter-spacing: var(--ls-caps);
    text-transform: uppercase;
    color: var(--text-3);
  }

  .table-spacer {
    position: relative;
  }

  .table-status {
    height: 100%;
  }
</style>
