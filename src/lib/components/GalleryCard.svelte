<script lang="ts">
  import type { RowRecord } from "../../api";
  import { showContextMenu } from "../context-menu.svelte";
  import { rowStore } from "../row-store.svelte";
  import { getSelectedCount, isRowSelected, toggleRow } from "../selection-store.svelte";
  import { thumbnails } from "../thumbnails";

  let {
    row,
    index,
    x,
    y,
    width,
    imageHeight,
  }: {
    row: RowRecord | undefined;
    index: number;
    x: number;
    y: number;
    width: number;
    imageHeight: number;
  } = $props();

  const hasImage = $derived(
    Boolean(row && (row.imagePath?.trim() || row.storedImagePath?.trim())),
  );
  const isActive = $derived(row != null && rowStore.activeRow?.id === row.id);
  const isChecked = $derived(row != null && isRowSelected(row.id));
  const selectionActive = $derived(getSelectedCount() > 0);

  let thumbUrl = $state<string | null>(null);
  let thumbFailed = $state(false);

  $effect(() => {
    thumbUrl = null;
    thumbFailed = false;
    const rowId = row?.id;
    if (rowId == null || !hasImage) {
      return;
    }
    let cancelled = false;
    thumbnails.load(rowId).then(
      url => {
        if (!cancelled) {
          thumbUrl = url;
        }
      },
      () => {
        if (!cancelled) {
          thumbFailed = true;
        }
      },
    );
    return () => {
      cancelled = true;
    };
  });

  const visibleTags = $derived(row?.tags.slice(0, 3) ?? []);
  const extraTagCount = $derived(Math.max(0, (row?.tags.length ?? 0) - 3));

  function onContextMenu(event: MouseEvent): void {
    if (!row) return;
    event.preventDefault();
    rowStore.activeRow = row;
    showContextMenu(row, event.clientX, event.clientY);
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
      onclick={() => (rowStore.activeRow = row ?? null)}
    >
      {#if thumbUrl}
        <img src={thumbUrl} alt="第 {row.sourceOrdinal} 行缩略图" loading="lazy" />
      {:else if !hasImage}
        <span class="thumb-note faint">无图片</span>
      {:else if thumbFailed}
        <span class="thumb-note faint">图片不可用</span>
      {:else}
        <span class="thumb-note faint">…</span>
      {/if}
    </button>
    <div class="footer">
      <span class="row-no faint">#{row.sourceOrdinal}</span>
      <span class="tags" title={row.tags.join(", ")}>
        {#each visibleTags as tag (tag)}
          <span class="chip">{tag}</span>
        {/each}
        {#if extraTagCount > 0}
          <span class="chip chip-more">+{extraTagCount}</span>
        {/if}
        {#if row.tags.length === 0}
          <span class="faint">—</span>
        {/if}
      </span>
    </div>
  {:else}
    <div class="thumb skeleton-block" style:height="{imageHeight}px"></div>
    <div class="footer">
      <span class="skeleton-line"></span>
    </div>
  {/if}
</div>

<style>
  .card {
    position: absolute;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    overflow: hidden;
    transition: border-color 0.1s ease, box-shadow 0.1s ease;
  }

  .card:hover:not(.is-skeleton) {
    border-color: var(--border-strong);
    box-shadow: var(--shadow-1);
  }

  .card.is-active {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent-soft-border);
  }

  .card.is-checked {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .select-box {
    position: absolute;
    top: 8px;
    left: 8px;
    z-index: 5;
    width: 18px;
    height: 18px;
    margin: 0;
    accent-color: var(--accent);
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.1s ease;
  }

  .card:hover .select-box,
  .card.is-checked .select-box,
  .card.selection-active .select-box {
    opacity: 1;
  }

  .thumb {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    border: none;
    padding: 0;
    background: var(--surface-2);
    overflow: hidden;
  }

  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .thumb-note {
    font-size: 12px;
  }

  .footer {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    min-height: 34px;
  }

  .row-no {
    font-size: 11px;
    flex: none;
  }

  .tags {
    display: flex;
    align-items: center;
    gap: 4px;
    overflow: hidden;
    font-size: 11px;
  }

  .chip {
    background: var(--accent-soft);
    color: var(--accent);
    border-radius: 999px;
    padding: 1px 8px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 110px;
  }

  .chip-more {
    background: var(--surface-2);
    color: var(--text-2);
    flex: none;
  }

  .skeleton-block {
    background: linear-gradient(100deg, var(--surface-2) 40%, var(--border) 50%, var(--surface-2) 60%);
    background-size: 200% 100%;
    animation: shimmer 1.2s infinite linear;
  }

  .skeleton-line {
    height: 10px;
    width: 60%;
    border-radius: 4px;
    background: var(--surface-2);
  }

  @keyframes shimmer {
    from {
      background-position: 200% 0;
    }
    to {
      background-position: -200% 0;
    }
  }
</style>
