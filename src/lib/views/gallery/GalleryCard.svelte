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

  const visibleTags = $derived(row?.tags.slice(0, 3) ?? []);
  const extraTagCount = $derived(Math.max(0, (row?.tags.length ?? 0) - 3));

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
    <div class="thumb shimmer" style:height="{imageHeight}px"></div>
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
    background: var(--glass-solid);
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    overflow: hidden;
    box-shadow: var(--shadow-lift-rest);
    transition:
      background var(--motion-fast) var(--ease-responsive),
      border-color var(--motion-fast) var(--ease-responsive),
      box-shadow var(--motion-lift) var(--ease-fluent),
      transform var(--motion-lift) var(--ease-fluent);
  }

  :global(html:not([data-glass="on"])) .card {
    background: var(--surface-opaque);
  }

  .card:hover:not(.is-skeleton) {
    border-color: var(--border-strong);
    box-shadow: var(--shadow-lift);
    transform: translateY(-2px);
  }

  .card:active:not(.is-skeleton) {
    transform: translateY(0) scale(0.99);
    box-shadow: var(--shadow-lift-rest);
    transition-duration: var(--motion-press);
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
    z-index: var(--z-nav);
    width: 18px;
    height: 18px;
    margin: 0;
    accent-color: var(--accent);
    cursor: pointer;
    opacity: 0;
    transform: scale(0.85);
    transition:
      opacity var(--motion-fast) var(--ease-responsive),
      transform var(--motion-fast) var(--ease-responsive);
  }

  .card:hover .select-box,
  .card.is-checked .select-box,
  .card.selection-active .select-box {
    opacity: 1;
    transform: scale(1);
  }

  .thumb {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    border: none;
    padding: 0;
    background: var(--surface-2);
    overflow: hidden;
  }

  .vibe-badge {
    position: absolute;
    top: 8px;
    right: 8px;
    z-index: 1;
    padding: 2px 7px;
    border-radius: var(--radius-m);
    background: var(--badge-vibe);
    box-shadow: 0 1px 3px rgb(25 18 45 / 24%);
    color: var(--on-accent);
    font-size: var(--font-xs);
    font-weight: 600;
    line-height: 1.4;
    letter-spacing: 0.3px;
    pointer-events: none;
  }

  .footer {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    min-height: 36px;
  }

  .row-no {
    font-size: var(--font-xs);
    flex: none;
  }

  .tags {
    display: flex;
    align-items: center;
    gap: 4px;
    overflow: hidden;
    font-size: var(--font-xs);
  }

  .skeleton-line {
    height: 10px;
    width: 60%;
    border-radius: var(--radius-s);
    background: var(--surface-2);
  }
</style>
