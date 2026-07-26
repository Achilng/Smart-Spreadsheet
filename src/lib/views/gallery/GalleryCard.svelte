<script lang="ts">
  import type { RowRecord } from "../../api";
  import { showContextMenu } from "../../stores/context-menu.svelte";
  import { beginFileDrag } from "../../stores/file-drag";
  import { rowStore } from "../../stores/row-store.svelte";
  import { getSelectedCount, isRowSelected, toggleRow } from "../../stores/selection-store.svelte";
  import Thumbnail from "../../ui/Thumbnail.svelte";
  import { vibeStatuses } from "../../images/vibe-statuses";

  let {
    row,
    index,
    x,
    y,
    width,
    imageHeight,
    enhance = false,
  }: {
    row: RowRecord | undefined;
    index: number;
    x: number;
    y: number;
    width: number;
    imageHeight: number;
    enhance?: boolean;
  } = $props();

  const hasImage = $derived(
    Boolean(row && (row.imagePath?.trim() || row.storedImagePath?.trim())),
  );
  const isActive = $derived(row != null && rowStore.activeRow?.id === row.id);
  const isChecked = $derived(row != null && isRowSelected(row.id));
  const selectionActive = $derived(getSelectedCount() > 0);

  const fileName = $derived(
    (row?.imagePath ?? row?.storedImagePath)?.split(/[\\/]/).pop() ?? null,
  );
  const resolution = $derived(
    row?.imageWidth && row?.imageHeight ? `${row.imageWidth} × ${row.imageHeight}` : null,
  );

  let dragging = $state(false);
  let vibeRefs = $state<number | null>(null);

  $effect(() => {
    vibeRefs = null;
    const current = row;
    if (!current || !hasImage) {
      return;
    }
    let cancelled = false;
    void vibeStatuses.load(current.id).then(
      count => {
        if (!cancelled) {
          vibeRefs = count;
        }
      },
      () => {},
    );
    return () => {
      cancelled = true;
    };
  });

  function onContextMenu(event: MouseEvent): void {
    if (!row) return;
    event.preventDefault();
    rowStore.activeRow = row;
    showContextMenu(row, event.clientX, event.clientY);
  }

  function onThumbMouseDown(event: MouseEvent): void {
    if (!row || !hasImage) return;
    beginFileDrag(event, row.id, () => { dragging = true; });
  }
</script>

<div
  class="card"
  class:is-active={isActive}
  class:is-checked={isChecked}
  class:is-skeleton={!row}
  class:selection-active={selectionActive}
  style:left="{x}px"
  style:top="{y}px"
  style:width="{width}px"
  role="listitem"
  oncontextmenu={onContextMenu}
>
  {#if row}
    <input
      type="checkbox"
      class="select-box"
      checked={isChecked}
      aria-label="选择第 {row.sourceOrdinal} 行"
      onclick={event => {
        const current = row;
        if (current) {
          toggleRow(current.id, index, event.shiftKey);
        }
      }}
    />
    <button
      type="button"
      class="thumb"
      style:height="{imageHeight}px"
      aria-label="查看第 {row.sourceOrdinal} 行详情"
      onmousedown={onThumbMouseDown}
      onclick={() => {
        if (dragging) { dragging = false; return; }
        rowStore.activeRow = row ?? null;
      }}
    >
      <Thumbnail
        rowId={row.id}
        {hasImage}
        alt="第 {row.sourceOrdinal} 行缩略图"
        {enhance}
        highPriority={isActive}
      />
      {#if vibeRefs}
        <span
          class="vibe-badge"
          title="原图元数据包含 {vibeRefs} 个 vibe 引用"
        >VIBE ×{vibeRefs}</span>
      {/if}
    </button>
    <div class="meta">
      <div class="meta-name" title={fileName ?? ""}>{fileName ?? `#${row.sourceOrdinal}`}</div>
      <div class="meta-sub tabular">{resolution ?? `#${row.sourceOrdinal}`}</div>
    </div>
  {:else}
    <div class="thumb shimmer" style:height="{imageHeight}px"></div>
    <div class="meta">
      <span class="skeleton-line shimmer"></span>
    </div>
  {/if}
</div>

<style>
  .card {
    position: absolute;
    display: flex;
    flex-direction: column;
  }

  /* 画册式：图片框单独承担圆角/阴影/抬升，图注裸排框下 */
  .thumb {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    border: none;
    padding: 0;
    background: var(--surface);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-1);
    overflow: hidden;
    transition:
      transform var(--motion-fast) var(--ease-responsive),
      box-shadow var(--motion-fast) var(--ease-responsive);
  }

  .card:hover:not(.is-skeleton) .thumb {
    transform: translateY(-2px);
    box-shadow: var(--shadow-hover);
  }

  .card:active:not(.is-skeleton) .thumb {
    transform: translateY(-2px) scale(0.99);
    transition-duration: var(--motion-press);
  }

  .card.is-active .thumb {
    outline: 2.5px solid var(--accent);
    outline-offset: 2px;
  }

  .card.is-checked:not(.is-active) .thumb {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .select-box {
    position: absolute;
    top: 8px;
    left: 8px;
    z-index: var(--z-nav);
    width: 18px;
    height: 18px;
    margin: 0;
    cursor: pointer;
    opacity: 0;
    transform: scale(0.85);
    transition:
      opacity var(--motion-fast) var(--ease-responsive),
      transform var(--motion-fast) var(--ease-responsive);
  }

  /* 卡片选中框走系统蓝（覆盖全局墨黑勾选底） */
  .select-box:checked {
    background: var(--accent);
    border-color: var(--accent);
  }

  .card:hover .select-box,
  .card.is-checked .select-box,
  .card.selection-active .select-box {
    opacity: 1;
    transform: scale(1);
  }

  .vibe-badge {
    position: absolute;
    top: 8px;
    right: 8px;
    z-index: 1;
  }

  .meta {
    padding: 8px 3px 0;
    min-width: 0;
  }

  .meta-name {
    font-size: var(--font-sm);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .meta-sub {
    margin-top: 1px;
    font-size: var(--font-xs);
    color: var(--text-3);
  }

  .skeleton-line {
    display: inline-block;
    height: 10px;
    width: 60%;
    border-radius: var(--radius-s);
    background: var(--surface-2);
  }
</style>
