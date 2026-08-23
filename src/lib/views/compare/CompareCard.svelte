<script lang="ts">
  import type { RowRecord } from "../../api";
  import { vibeStatuses } from "../../images/vibe-statuses";
  import Thumbnail from "../../ui/Thumbnail.svelte";
  import { rowFileName, rowResolution } from "../../utils/row-display";
  import { modelVersionBadge } from "../../utils/model-version";

  let {
    row,
    onactivate,
  }: {
    row: RowRecord;
    onactivate: () => void;
  } = $props();

  const hasImage = $derived(
    Boolean(row.imagePath?.trim() || row.storedImagePath?.trim()),
  );
  const label = $derived(
    rowFileName(row) ?? row.artists?.split("\n")[0]?.trim() ?? `#${row.sourceOrdinal}`,
  );
  const resolution = $derived(rowResolution(row));
  const versionBadge = $derived(modelVersionBadge(row.generationModel));

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
  class="compare-card"
  title={[rowFileName(row), resolution, row.imagePath].filter(Boolean).join("\n") || `#${row.sourceOrdinal}`}
  onclick={onactivate}
>
  <span class="thumb">
    <Thumbnail rowId={row.id} {hasImage} alt={label} />
    {#if versionBadge}
      <span
        class="version-badge {versionBadge.className}"
        title={"作画模型：" + (row.generationModel ?? "")}
      >{versionBadge.label}</span>
    {/if}
    {#if vibeRefs}
      <span
        class="vibe-badge"
        title="原图元数据包含 {vibeRefs} 个 vibe 引用"
      >VIBE ×{vibeRefs}</span>
    {/if}
  </span>
  <span class="card-label">{label}</span>
  {#if resolution}
    <span class="card-sub tabular">{resolution}</span>
  {/if}
</button>

<style>
  .compare-card {
    display: flex;
    flex-direction: column;
    width: 128px;
    padding: 0;
    border: none;
    background: transparent;
    cursor: pointer;
    text-align: center;
  }

  .thumb {
    position: relative;
    width: 100%;
    height: 128px;
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

  .compare-card:hover .thumb {
    transform: translateY(-2px);
    box-shadow: var(--shadow-hover);
  }

  .compare-card:active .thumb {
    transform: translateY(-1px) scale(0.985);
    transition-duration: var(--motion-press);
  }

  .vibe-badge {
    position: absolute;
    top: 6px;
    right: 6px;
    z-index: 1;
  }

  .version-badge {
    position: absolute;
    top: 6px;
    left: 6px;
    z-index: 1;
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
  }

  .card-sub {
    display: block;
    padding: 1px 3px 0;
    font-size: var(--font-xs);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
</style>
