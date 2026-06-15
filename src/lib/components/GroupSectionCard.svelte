<script lang="ts">
  import type { RowRecord } from "../../api";
  import { rowStore } from "../row-store.svelte";
  import { thumbnails } from "../thumbnails";

  let { row, onactivate }: { row: RowRecord; onactivate: () => void } = $props();

  const hasImage = $derived(
    Boolean(row.imagePath?.trim() || row.storedImagePath?.trim()),
  );
  const isActive = $derived(rowStore.activeRow?.id === row.id);

  let thumbUrl = $state<string | null>(null);
  let thumbFailed = $state(false);

  $effect(() => {
    thumbUrl = null;
    thumbFailed = false;
    if (!hasImage) return;
    let cancelled = false;
    thumbnails.load(row.id).then(
      url => { if (!cancelled) thumbUrl = url; },
      () => { if (!cancelled) thumbFailed = true; },
    );
    return () => { cancelled = true; };
  });

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
>
  <div class="thumb">
    {#if thumbUrl}
      <img src={thumbUrl} alt="#{row.sourceOrdinal}" loading="lazy" />
    {:else if !hasImage}
      <span class="thumb-note faint">无图</span>
    {:else if thumbFailed}
      <span class="thumb-note faint">不可用</span>
    {:else}
      <span class="thumb-note faint">…</span>
    {/if}
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

  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .thumb-note {
    font-size: 11px;
  }

  .card-label {
    display: block;
    padding: 4px 6px;
    font-size: 11px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    text-align: center;
  }
</style>
