<script lang="ts">
  import type { RowRecord } from "../../api";
  import { PAGE_SIZE, ensurePage, getRow, resetRows, rowStore } from "../row-store.svelte";
  import { thumbnails } from "../thumbnails";
  import GalleryCard from "./GalleryCard.svelte";

  const GAP = 12;
  const PADDING = 16;
  const MIN_CARD_WIDTH = 190;
  const FOOTER_HEIGHT = 34;
  const OVERSCAN_ROWS = 2;

  let viewport = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewportWidth = $state(0);
  let viewportHeight = $state(0);

  const columns = $derived(
    Math.max(1, Math.floor((viewportWidth - PADDING * 2 + GAP) / (MIN_CARD_WIDTH + GAP))),
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
    if (rowStore.totalCount === 0 || viewportWidth <= 0 || viewportHeight <= 0) {
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

  // 可见区域变化时：补加载缺页，并让缩略图队列丢弃已滚出视野的请求
  $effect(() => {
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
    for (const pageIndex of missingPages) {
      ensurePage(pageIndex);
    }
  });

  // 任何整体重载（筛选变化、批量操作、工作簿替换）都回到顶部
  $effect(() => {
    if (rowStore.initialLoading) {
      scrollTop = 0;
      if (viewport) {
        viewport.scrollTop = 0;
      }
    }
  });
</script>

<div
  class="gallery-viewport"
  bind:this={viewport}
  bind:clientWidth={viewportWidth}
  bind:clientHeight={viewportHeight}
  onscroll={() => (scrollTop = viewport?.scrollTop ?? 0)}
>
  {#if rowStore.error}
    <div class="gallery-status">
      <p class="muted">加载失败：{rowStore.error}</p>
      <button type="button" class="btn" onclick={() => resetRows()}>重试</button>
    </div>
  {:else if rowStore.initialLoading}
    <div class="gallery-status">
      <p class="muted">正在加载…</p>
    </div>
  {:else if rowStore.totalCount === 0}
    <div class="gallery-status">
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
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
  }
</style>
