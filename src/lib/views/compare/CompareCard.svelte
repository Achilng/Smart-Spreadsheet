<script lang="ts">
  import type { RowRecord } from "../../api";
  import { beginFileDrag } from "../../stores/file-drag";
  import Thumbnail from "../../ui/Thumbnail.svelte";
  import { modelVersionBadge } from "../../utils/model-version";
  import { rowFileName, rowResolution } from "../../utils/row-display";

  let {
    row,
    onactivate,
  }: {
    row: RowRecord;
    onactivate: (row: RowRecord) => void;
  } = $props();

  const hasImage = $derived(
    Boolean(row.imagePath?.trim() || row.storedImagePath?.trim()),
  );
  const fileName = $derived(rowFileName(row) ?? `#${row.sourceOrdinal}`);
  const resolution = $derived(rowResolution(row));
  const versionBadge = $derived(modelVersionBadge(row.generationModel));

  let dragging = $state(false);

  function onThumbMouseDown(event: MouseEvent): void {
    if (!hasImage) return;
    beginFileDrag(
      event,
      row.id,
      () => {
        dragging = true;
      },
      // 外拖结束后主动复位，避免松手后的点击误触发并排对比
      () => {
        dragging = false;
      },
    );
  }

  function activate(): void {
    if (dragging) {
      dragging = false;
      return;
    }
    onactivate(row);
  }
</script>

<button
  type="button"
  class="compare-card"
  title={`${fileName}${resolution ? ` · ${resolution}` : ""}`}
  onmousedown={onThumbMouseDown}
  onclick={activate}
>
  <span class="thumb">
    <Thumbnail rowId={row.id} {hasImage} alt={fileName} />
    {#if versionBadge}
      <span class="version-badge {versionBadge.className}" title={"作画模型：" + (row.generationModel ?? "")}>
        {versionBadge.label}
      </span>
    {/if}
    {#if row.vibeReferenceCount}
      <span class="vibe-badge" title="包含 {row.vibeReferenceCount} 个 VIBE 引用">VIBE ×{row.vibeReferenceCount}</span>
    {/if}
  </span>
  <span class="meta">
    <span class="meta-name">{fileName}</span>
    {#if resolution}
      <span class="meta-sub tabular">{resolution}</span>
    {/if}
  </span>
</button>

<style>
  .compare-card {
    display: flex;
    flex-direction: column;
    border: none;
    padding: 0;
    background: transparent;
    cursor: pointer;
    text-align: left;
    font: inherit;
    color: inherit;
    min-width: 0;
  }

  .thumb {
    position: relative;
    display: block;
    width: 100%;
    aspect-ratio: 1;
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
    transform: translateY(-2px) scale(0.99);
    transition-duration: var(--motion-press);
  }

  .compare-card:focus-visible {
    outline: 2.5px solid var(--accent);
    outline-offset: 2px;
    border-radius: var(--radius-m);
  }

  .version-badge {
    position: absolute;
    top: 8px;
    left: 8px;
    z-index: 1;
  }

  .vibe-badge {
    position: absolute;
    top: 8px;
    right: 8px;
    z-index: 1;
  }

  .meta {
    display: flex;
    flex-direction: column;
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
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
