<script lang="ts">
  import type { RowRecord } from "../../api";
  import { rowStore } from "../row-store.svelte";
  import { getSelectedCount, isRowSelected, toggleRow } from "../selection-store.svelte";
  import Thumbnail from "./Thumbnail.svelte";

  let {
    row,
    index,
    y,
    height,
  }: {
    row: RowRecord | undefined;
    index: number;
    y: number;
    height: number;
  } = $props();

  const hasImage = $derived(
    Boolean(row && (row.imagePath?.trim() || row.storedImagePath?.trim())),
  );
  const isActive = $derived(row != null && rowStore.activeRow?.id === row.id);
  const isChecked = $derived(row != null && isRowSelected(row.id));
  const selectionActive = $derived(getSelectedCount() > 0);

  const visibleTags = $derived(row?.tags.slice(0, 3) ?? []);
  const extraTagCount = $derived(Math.max(0, (row?.tags.length ?? 0) - 3));

  function onRowClick(): void {
    if (row) {
      rowStore.activeRow = row;
    }
  }

  function onRowKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onRowClick();
    }
  }
</script>

<div
  class="table-row"
  class:is-active={isActive}
  class:is-checked={isChecked}
  class:selection-active={selectionActive}
  class:is-skeleton={!row}
  style:transform="translateY({y}px)"
  style:height="{height}px"
  role="row"
  tabindex={row ? 0 : -1}
  aria-selected={isChecked}
  onclick={onRowClick}
  onkeydown={onRowKeydown}
>
  {#if row}
    <div class="cell cell-check">
      <input
        type="checkbox"
        checked={isChecked}
        aria-label="选择第 {row.sourceOrdinal} 行"
        onclick={event => {
          event.stopPropagation();
          const current = row;
          if (current) {
            toggleRow(current.id, index, event.shiftKey);
          }
        }}
      />
    </div>
    <div class="cell cell-thumb">
      <Thumbnail rowId={row.id} {hasImage} alt="第 {row.sourceOrdinal} 行缩略图" />
    </div>
    <div class="cell cell-rowno faint">#{row.sourceOrdinal}</div>
    <div class="cell cell-time" title={row.time ?? ""}>{row.time ?? "—"}</div>
    <div class="cell cell-prompt" title={row.positivePrompt ?? ""}>
      {row.positivePrompt ?? "—"}
    </div>
    <div class="cell cell-artists" title={row.artists ?? ""}>{row.artists ?? "—"}</div>
    <div class="cell cell-tags" title={row.tags.join(", ")}>
      {#each visibleTags as tag (tag)}
        <span class="chip">{tag}</span>
      {/each}
      {#if extraTagCount > 0}
        <span class="chip chip-more">+{extraTagCount}</span>
      {/if}
      {#if row.tags.length === 0}
        <span class="faint">—</span>
      {/if}
    </div>
  {:else}
    <div class="cell cell-check"></div>
    <div class="cell cell-thumb"><span class="skeleton-line"></span></div>
    <div class="cell"><span class="skeleton-line"></span></div>
    <div class="cell"><span class="skeleton-line"></span></div>
    <div class="cell"><span class="skeleton-line wide"></span></div>
    <div class="cell"><span class="skeleton-line"></span></div>
    <div class="cell"><span class="skeleton-line"></span></div>
  {/if}
</div>

<style>
  .table-row {
    position: absolute;
    left: 0;
    right: 0;
    display: grid;
    grid-template-columns: 36px 76px 64px 150px minmax(0, 2.2fr) minmax(0, 1.2fr) minmax(0, 1.4fr);
    align-items: center;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    cursor: default;
  }

  .table-row:hover:not(.is-skeleton) {
    background: var(--surface-2);
  }

  .table-row.is-checked {
    background: var(--accent-soft);
  }

  .table-row.is-active {
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .table-row:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .cell {
    padding: 0 10px;
    min-width: 0;
    font-size: 12.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cell-check {
    display: flex;
    justify-content: center;
    padding: 0;
  }

  .cell-check input {
    width: 15px;
    height: 15px;
    margin: 0;
    accent-color: var(--accent);
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.1s ease;
  }

  .table-row:hover .cell-check input,
  .table-row.is-checked .cell-check input,
  .table-row.selection-active .cell-check input {
    opacity: 1;
  }

  .cell-thumb {
    height: 48px;
    width: 64px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--surface-2);
    border-radius: 4px;
    margin: 0 6px;
  }

  .cell-rowno {
    font-size: 11.5px;
  }

  .cell-time {
    color: var(--text-2);
  }

  .cell-prompt,
  .cell-artists {
    color: var(--text-2);
  }

  .cell-tags {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .chip {
    background: var(--accent-soft);
    color: var(--accent);
    border-radius: 999px;
    padding: 1px 8px;
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 110px;
    flex: none;
  }

  .table-row.is-checked .chip {
    background: var(--surface);
  }

  .chip-more {
    background: var(--surface-2);
    color: var(--text-2);
  }

  .skeleton-line {
    display: inline-block;
    height: 10px;
    width: 70%;
    border-radius: 4px;
    background: var(--surface-2);
  }

  .skeleton-line.wide {
    width: 90%;
  }
</style>
