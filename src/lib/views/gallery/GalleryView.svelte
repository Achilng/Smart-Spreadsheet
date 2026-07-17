<script lang="ts">
  import { onDestroy } from "svelte";

  import type { RowRecord } from "../../api";
  import { app } from "../../stores/app-state.svelte";
  import { PAGE_SIZE, ensurePage, getRow, resetRows, rowStore } from "../../stores/row-store.svelte";
  import { thumbnails } from "../../images/thumbnails";
  import { galleryPreviews } from "../../images/progressive-images";
  import { vibeStatuses } from "../../images/vibe-statuses";
  import { restoreScrollPosition, saveScrollPosition, scrollPositionVersion } from "../../stores/view-state";
  import GalleryCard from "./GalleryCard.svelte";

  let { active = true }: { active?: boolean } = $props();

  const GAP = 12;
  const PADDING = 16;
  const FOOTER_HEIGHT = 34;
  const OVERSCAN_ROWS = 2;
  const PROGRESSIVE_DELAY_MS = 400;

  const minCardWidth = $derived(app.galleryCardSize);

  let viewport = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let measuredWidth = $state(0);
  let measuredHeight = $state(0);
  // 面板隐藏（display:none）时 ResizeObserver 会报 0，若让 0 流入布局计算，
  // 列数/卡宽退化会把 spacer 改写成错误高度，切回第一帧浏览器就按它钳制
  // scrollTop。因此冻结最后一次有效尺寸，隐藏期间布局保持不变。
  let viewportWidth = $state(0);
  let viewportHeight = $state(0);
  let progressiveReady = $state(false);
  let progressiveTimer: ReturnType<typeof setTimeout> | undefined;
  let lastScrollSaveVersion = scrollPositionVersion();

  $effect(() => {
    if (measuredWidth > 0) {
      viewportWidth = measuredWidth;
    }
    if (measuredHeight > 0) {
      viewportHeight = measuredHeight;
    }
  });

  const columns = $derived(
    Math.max(1, Math.floor((viewportWidth - PADDING * 2 + GAP) / (minCardWidth + GAP))),
  );
  const cardWidth = $derived(
    Math.max(1, Math.floor((viewportWidth - PADDING * 2 - GAP * (columns - 1)) / columns)),
  );
  const imageHeight = $derived(cardWidth);
  const cellHeight = $derived(imageHeight + FOOTER_HEIGHT + GAP);
  const gridRows = $derived(Math.ceil(rowStore.totalCount / columns));
  const spacerHeight = $derived(
    gridRows === 0 ? 0 : PADDING * 2 + gridRows * cellHeight - GAP,
  );

  interface Cell {
    index: number;
    row: RowRecord | undefined;
    x: number;
    y: number;
  }

  const cells = $derived.by(() => {
    void rowStore.pagesVersion;
    if (!active || rowStore.totalCount === 0 || viewportWidth <= 0 || viewportHeight <= 0) {
      return [] as Cell[];
    }
    const firstRow = Math.max(0, Math.floor((scrollTop - PADDING) / cellHeight) - OVERSCAN_ROWS);
    const lastRow = Math.min(
      gridRows,
      Math.ceil((scrollTop - PADDING + viewportHeight) / cellHeight) + OVERSCAN_ROWS,
    );
    const result: Cell[] = [];
    for (let gridRow = firstRow; gridRow < lastRow; gridRow += 1) {
      for (let column = 0; column < columns; column += 1) {
        const index = gridRow * columns + column;
        if (index >= rowStore.totalCount) {
          break;
        }
        result.push({
          index,
          row: getRow(index),
          x: PADDING + column * (cardWidth + GAP),
          y: PADDING + gridRow * cellHeight,
        });
      }
    }
    return result;
  });

  const progressiveRowIds = $derived.by(() => {
    void rowStore.pagesVersion;
    const rowIds = new Set<number>();
    if (!active || viewportWidth <= 0 || viewportHeight <= 0 || rowStore.totalCount === 0) {
      return rowIds;
    }
    const firstRow = Math.max(0, Math.floor((scrollTop - PADDING) / cellHeight));
    const lastRow = Math.min(
      gridRows,
      Math.ceil((scrollTop - PADDING + viewportHeight) / cellHeight),
    );
    for (let gridRow = firstRow; gridRow < lastRow; gridRow += 1) {
      for (let column = 0; column < columns; column += 1) {
        const index = gridRow * columns + column;
        if (index >= rowStore.totalCount) {
          break;
        }
        const row = getRow(index);
        if (row) {
          rowIds.add(row.id);
        }
      }
    }
    return rowIds;
  });

  function scheduleProgressiveLoading(): void {
    if (progressiveTimer) {
      clearTimeout(progressiveTimer);
    }
    progressiveTimer = setTimeout(() => {
      progressiveTimer = undefined;
      if (active) {
        progressiveReady = true;
      }
    }, PROGRESSIVE_DELAY_MS);
  }

  function pauseProgressiveLoading(): void {
    progressiveReady = false;
    scheduleProgressiveLoading();
  }

  $effect(() => {
    if (!active) {
      progressiveReady = false;
      if (progressiveTimer) {
        clearTimeout(progressiveTimer);
        progressiveTimer = undefined;
      }
      galleryPreviews.retain(new Set());
      return;
    }
    scheduleProgressiveLoading();
    return () => {
      if (progressiveTimer) {
        clearTimeout(progressiveTimer);
        progressiveTimer = undefined;
      }
    };
  });

  $effect(() => {
    galleryPreviews.retain(progressiveReady && active ? progressiveRowIds : new Set());
  });

  // 可见区域变化时：补加载缺页，并让缩略图队列丢弃已滚出视野的请求
  $effect(() => {
    if (!active || viewportWidth <= 0 || viewportHeight <= 0) {
      return;
    }
    const missingPages = new Set<number>();
    const visibleRowIds = new Set<number>();
    for (const cell of cells) {
      if (cell.row) {
        visibleRowIds.add(cell.row.id);
      } else {
        missingPages.add(Math.floor(cell.index / PAGE_SIZE));
      }
    }
    thumbnails.retain(visibleRowIds);
    vibeStatuses.retain(visibleRowIds);
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
      viewportWidth <= 0 ||
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
      "gallery",
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
    pauseProgressiveLoading();
    // 恢复期间的钳制事件和隐藏状态下的读数不代表用户位置，不能写入记忆
    if (active && !restoring) {
      saveGalleryScroll(scrollTop);
    }
  }

  function saveGalleryScroll(top: number): void {
    saveScrollPosition("gallery", top);
    lastScrollSaveVersion = scrollPositionVersion();
  }

  onDestroy(() => {
    if (progressiveTimer) {
      clearTimeout(progressiveTimer);
    }
    galleryPreviews.retain(new Set());
    // 隐藏状态下 scrollTop 恒为 0，只有激活状态的读数才可信
    if (active && viewport && lastScrollSaveVersion === scrollPositionVersion()) {
      saveGalleryScroll(viewport.scrollTop);
    }
  });
</script>

<div
  class="gallery-viewport"
  bind:this={viewport}
  bind:clientWidth={measuredWidth}
  bind:clientHeight={measuredHeight}
  onscroll={onScroll}
>
  {#if rowStore.error}
    <div class="gallery-status empty-state">
      <p class="muted">加载失败：{rowStore.error}</p>
      <button type="button" class="btn" onclick={() => resetRows()}>重试</button>
    </div>
  {:else if rowStore.initialLoading}
    <div class="gallery-status empty-state">
      <p class="muted">正在加载…</p>
    </div>
  {:else if rowStore.totalCount === 0}
    <div class="gallery-status empty-state">
      <p class="muted">
        {rowStore.tags.length > 0 ? "当前筛选没有匹配记录。" : "工作簿没有数据行。"}
      </p>
    </div>
  {:else}
    <div class="gallery-spacer" style:height="{spacerHeight}px">
      {#each cells as cell (cell.index)}
        <GalleryCard
          row={cell.row}
          index={cell.index}
          x={cell.x}
          y={cell.y}
          width={cardWidth}
          {imageHeight}
          enhance={progressiveReady && Boolean(cell.row && progressiveRowIds.has(cell.row.id))}
        />
      {/each}
    </div>
  {/if}
</div>

<style>
  .gallery-viewport {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    position: relative;
  }

  .gallery-spacer {
    position: relative;
  }

  .gallery-status {
    height: 100%;
  }
</style>
