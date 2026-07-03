<script lang="ts">
  import type { RowRecord } from "../../api";
  import { rowStore } from "../../stores/row-store.svelte";
  import Thumbnail from "../../ui/Thumbnail.svelte";

  import { showContextMenu } from "../../stores/context-menu.svelte";

  let { row, onactivate }: { row: RowRecord; onactivate: () => void } = $props();

  function onContextMenu(event: MouseEvent): void {
    event.preventDefault();
    showContextMenu(row, event.clientX, event.clientY);
  }

  const hasImage = $derived(
    Boolean(row.imagePath?.trim() || row.storedImagePath?.trim()),
  );
  const isActive = $derived(rowStore.activeRow?.id === row.id);

  const label = $derived(
    row.artists?.split("\n")[0]?.trim() || `#${row.sourceOrdinal}`,
  );
</script>

<button
  type="button"
  class="section-card"
  class:is-active={isActive}
  title={row.artists || row.positivePrompt?.slice(0, 80) || `#${row.sourceOrdinal}`}
  onclick={onactivate}
  oncontextmenu={onContextMenu}
>
  <div class="thumb">
    <Thumbnail rowId={row.id} {hasImage} alt={label} />
  </div>
  <span class="card-label">{label}</span>
</button>

<style>
  .section-card {
    display: flex;
    flex-direction: column;
    width: 120px;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--surface);
    overflow: hidden;
    cursor: pointer;
    padding: 0;
    transition: border-color 0.1s ease;
  }

  .section-card:hover {
    border-color: var(--border-strong);
  }

  .section-card.is-active {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent-soft-border);
  }

  .thumb {
    width: 100%;
    height: 120px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--surface-2);
    overflow: hidden;
  }

  .card-label {
    display: block;
    padding: 4px 6px;
    font-size: var(--font-xs);
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    text-align: center;
  }
</style>
