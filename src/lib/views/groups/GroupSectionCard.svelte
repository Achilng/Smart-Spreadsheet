<script lang="ts">
  import type { RowRecord } from "../../api";
  import { vibeStatuses } from "../../images/vibe-statuses";
  import { rowStore } from "../../stores/row-store.svelte";
  import Thumbnail from "../../ui/Thumbnail.svelte";
  import CardTagSummary from "../../ui/CardTagSummary.svelte";
  import { rowFileName, rowResolution } from "../../utils/row-display";

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

  // 重复项对比需要文件名与分辨率做决策依据；无文件名时退回画师串
  const label = $derived(
    rowFileName(row) ?? row.artists?.split("\n")[0]?.trim() ?? `#${row.sourceOrdinal}`,
  );
  const resolution = $derived(rowResolution(row));

  let vibeRefs = $state<number | null>(null);

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

<button
  type="button"
  class="section-card"
  class:is-active={isActive}
  title={[rowFileName(row), resolution, row.imagePath].filter(Boolean).join("\n") || `#${row.sourceOrdinal}`}
  onclick={onactivate}
  oncontextmenu={onContextMenu}
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

<style>
  .section-card {
    display: flex;
    flex-direction: column;
    width: 120px;
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
