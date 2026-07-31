<script lang="ts">
  import type { RowRecord } from "../../api";
  import { vibeStatuses } from "../../images/vibe-statuses";
  import { showContextMenu } from "../../stores/context-menu.svelte";
  import { beginFileDrag } from "../../stores/file-drag";
  import { rowStore } from "../../stores/row-store.svelte";
  import {
    getSelectedCount,
    isRowSelected,
    modifierSelectOrdered,
    toggleOrderedRow,
  } from "../../stores/selection-store.svelte";
  import Thumbnail from "../../ui/Thumbnail.svelte";
  import CardTagSummary from "../../ui/CardTagSummary.svelte";
  import { rowFileName, rowResolution } from "../../utils/row-display";

  let {
    row,
    selectionOrder,
    selectionScope,
    onactivate,
  }: {
    row: RowRecord;
    selectionOrder: readonly number[];
    selectionScope: "groups" | "duplicates";
    onactivate: () => void;
  } = $props();

  function onContextMenu(event: MouseEvent): void {
    event.preventDefault();
    rowStore.activeRow = row;
    showContextMenu(row, event.clientX, event.clientY);
  }

  const hasImage = $derived(
    Boolean(row.imagePath?.trim() || row.storedImagePath?.trim()),
  );
  const isActive = $derived(rowStore.activeRow?.id === row.id);
  const isChecked = $derived(isRowSelected(row.id));
  const selectionActive = $derived(getSelectedCount() > 0);

  // 重复项对比需要文件名与分辨率做决策依据；无文件名时退回画师串
  const label = $derived(
    rowFileName(row) ?? row.artists?.split("\n")[0]?.trim() ?? `#${row.sourceOrdinal}`,
  );
  const resolution = $derived(rowResolution(row));

  let dragging = $state(false);
  let vibeRefs = $state<number | null>(null);

  function onCardMouseDown(event: MouseEvent): void {
    if (!hasImage || !(event.target instanceof Element) || !event.target.closest(".thumb")) {
      return;
    }
    beginFileDrag(
      event,
      row.id,
      () => { dragging = true; },
      () => { dragging = false; },
    );
  }

  function onCardClick(event: MouseEvent): void {
    if (dragging) {
      dragging = false;
      return;
    }
    if (modifierSelectOrdered(row.id, selectionOrder, selectionScope, event)) {
      return;
    }
    onactivate();
  }

  $effect(() => {
    vibeRefs = null;
    const rowId = row.id;
    if (!hasImage) {
      return;
    }
    let cancelled = false;
    void vibeStatuses.load(rowId).then(
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
</script>

<div
  class="section-card"
  class:is-active={isActive}
  class:is-checked={isChecked}
  class:selection-active={selectionActive}
  title={[rowFileName(row), resolution, row.imagePath].filter(Boolean).join("\n") || `#${row.sourceOrdinal}`}
  oncontextmenu={onContextMenu}
  role="listitem"
>
  <input
    type="checkbox"
    class="select-box"
    checked={isChecked}
    aria-label="选择第 {row.sourceOrdinal} 行"
    onclick={event => {
      event.stopPropagation();
      toggleOrderedRow(row.id, selectionOrder, selectionScope, event.shiftKey);
    }}
  />
  <button
    type="button"
    class="card-main"
    aria-label="查看第 {row.sourceOrdinal} 行详情"
    onmousedown={onCardMouseDown}
    onclick={onCardClick}
  >
    <div class="thumb">
      <Thumbnail rowId={row.id} {hasImage} alt={label} />
      {#if row.tags.length > 0}
        <span class="tag-overlay"><CardTagSummary tags={row.tags} /></span>
      {/if}
      {#if vibeRefs}
        <span
          class="vibe-badge"
          title="原图元数据包含 {vibeRefs} 个 vibe 引用"
        >VIBE ×{vibeRefs}</span>
      {/if}
    </div>
    <span class="card-label">{label}</span>
    {#if resolution}
      <span class="card-sub tabular">{resolution}</span>
    {/if}
  </button>
</div>

<style>
  .section-card {
    position: relative;
    display: flex;
    flex-direction: column;
    width: 120px;
    padding: 0;
  }

  .card-main {
    display: flex;
    flex-direction: column;
    width: 100%;
    border: none;
    background: transparent;
    cursor: pointer;
    padding: 0;
  }

  .thumb {
    position: relative;
    width: 100%;
    height: 120px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--surface);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-1);
    overflow: hidden;
    transition:
      transform var(--motion-fast) var(--ease-responsive),
      box-shadow var(--motion-fast) var(--ease-responsive);
  }

  .section-card:hover .thumb {
    transform: translateY(-2px);
    box-shadow: var(--shadow-hover);
  }

  .section-card:active .thumb {
    transform: translateY(-1px) scale(0.985);
    transition-duration: var(--motion-press);
  }

  .section-card.is-active .thumb {
    outline: 2.5px solid var(--accent);
    outline-offset: 2px;
  }

  .vibe-badge {
    position: absolute;
    top: 6px;
    right: 6px;
    z-index: 1;
  }

  .section-card.is-checked:not(.is-active) .thumb {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .select-box {
    position: absolute;
    top: 6px;
    left: 6px;
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

  .select-box:checked {
    background: var(--accent);
    border-color: var(--accent);
  }

  .section-card:hover .select-box,
  .section-card.is-checked .select-box,
  .section-card.selection-active .select-box,
  .select-box:focus-visible {
    opacity: 1;
    transform: scale(1);
  }

  .tag-overlay {
    position: absolute;
    left: 6px;
    right: 6px;
    bottom: 6px;
    z-index: 1;
    display: flex;
    min-width: 0;
  }

  .card-label {
    display: block;
    padding: 6px 3px 0;
    font-size: var(--font-xs);
    font-weight: 600;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    text-align: center;
  }

  .card-sub {
    display: block;
    padding: 1px 3px 0;
    font-size: var(--font-xs);
    color: var(--text-3);
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
</style>
