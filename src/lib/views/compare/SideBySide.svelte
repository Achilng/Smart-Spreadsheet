<script lang="ts">
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import Crosshair from "@lucide/svelte/icons/crosshair";
  import { emitTo } from "@tauri-apps/api/event";
  import type { RowRecord } from "../../api";
  import { closeSideBySide } from "../../stores/compare-store.svelte";
  import { diffPromptField } from "../../utils/prompt-diff";
  import { modelVersionBadge } from "../../utils/model-version";
  import { focusMainWindow } from "../../windows/toolbox";
  import { rowFileName, rowResolution } from "../../utils/row-display";
  import PaneImage from "./PaneImage.svelte";

  let {
    sample,
    target,
  }: {
    sample: RowRecord;
    target: RowRecord;
  } = $props();

  const diff = $derived(diffPromptField(sample.positivePrompt, target.positivePrompt));

  interface ParamRow {
    label: string;
    sampleValue: string;
    targetValue: string;
    different: boolean;
  }

  function paramValue(value: string | number | null | undefined): string {
    if (value == null || value === "") {
      return "—";
    }
    return String(value);
  }

  const paramRows = $derived.by(() => {
    const rows: ParamRow[] = [
      { label: "模型", sampleValue: paramValue(sample.generationModel), targetValue: paramValue(target.generationModel), different: false },
      { label: "采样器", sampleValue: paramValue(sample.generationSampler), targetValue: paramValue(target.generationSampler), different: false },
      { label: "步数", sampleValue: paramValue(sample.generationSteps), targetValue: paramValue(target.generationSteps), different: false },
      { label: "种子", sampleValue: paramValue(sample.generationSeed), targetValue: paramValue(target.generationSeed), different: false },
      { label: "Guidance", sampleValue: paramValue(sample.generationScale), targetValue: paramValue(target.generationScale), different: false },
      { label: "CFG Rescale", sampleValue: paramValue(sample.generationCfgRescale), targetValue: paramValue(target.generationCfgRescale), different: false },
      { label: "噪声调度", sampleValue: paramValue(sample.generationNoiseSchedule), targetValue: paramValue(target.generationNoiseSchedule), different: false },
      { label: "尺寸", sampleValue: paramValue(rowResolution(sample)), targetValue: paramValue(rowResolution(target)), different: false },
    ];
    for (const row of rows) {
      row.different = row.sampleValue !== row.targetValue;
    }
    return rows;
  });

  async function locateInGallery(rowId: number): Promise<void> {
    try {
      // 复用以图搜图的跨窗口定位通道：主窗口监听同一事件并定位画廊。
      await emitTo("main", "toolbox://open-row", { rowId });
      await focusMainWindow();
    } catch {
      // 主窗口可能已关闭；忽略即可。
    }
  }

  const panes = $derived([
    { caption: "样本", row: sample },
    { caption: "目标", row: target },
  ]);
</script>

{#snippet pane(caption: string, row: RowRecord)}
  {@const hasImage = Boolean(row.imagePath?.trim() || row.storedImagePath?.trim())}
  {@const versionBadge = modelVersionBadge(row.generationModel)}
  <figure class="pane">
    <div class="pane-media">
      {#if hasImage}
        <PaneImage rowId={row.id} {hasImage} alt={rowFileName(row) ?? caption} />
      {:else}
        <span class="media-note">无图</span>
      {/if}
      {#if versionBadge}
        <span
          class="version-badge {versionBadge.className}"
          title={"作画模型：" + (row.generationModel ?? "")}
        >{versionBadge.label}</span>
      {/if}
    </div>
    <figcaption class="pane-caption">
      <span class="pane-kind">{caption}</span>
      <span class="pane-name" title={row.imagePath ?? undefined}>
        {rowFileName(row) ?? `#${row.sourceOrdinal}`}
      </span>
      <button
        type="button"
        class="locate"
        onclick={() => void locateInGallery(row.id)}
        title="恢复主窗口并在画廊中定位这张图"
      >
        <Crosshair size={13} strokeWidth={2} />
        在主画廊定位
      </button>
    </figcaption>
  </figure>
{/snippet}

<div class="side-by-side">
  <header class="head">
    <button type="button" class="back" onclick={closeSideBySide}>
      <ArrowLeft size={15} strokeWidth={2} />
      返回分区列表
    </button>
  </header>

  <div class="panes">
    {#each panes as paneData (paneData.caption)}
      {@render pane(paneData.caption, paneData.row)}
    {/each}
  </div>

  <section class="diff-block">
    <h2 class="block-title">提示词差异</h2>
    {#if diff.onlyLeft.length === 0 && diff.shared.length === 0 && diff.onlyRight.length === 0}
      <div class="block-empty">两侧正向提示词都为空。</div>
    {:else}
      <div class="diff-columns">
        <div class="diff-column">
          <h3 class="diff-head">仅样本有 <span class="diff-count">{diff.onlyLeft.length}</span></h3>
          {#if diff.onlyLeft.length}
            <div class="token-list">
              {#each diff.onlyLeft as token, index (index)}
                <span class="token" class:quality={token.isQuality}>{token.display}</span>
              {/each}
            </div>
          {:else}
            <div class="diff-empty">无</div>
          {/if}
        </div>
        <div class="diff-column">
          <h3 class="diff-head">双方共有 <span class="diff-count">{diff.shared.length}</span></h3>
          {#if diff.shared.length}
            <div class="token-list">
              {#each diff.shared as token, index (index)}
                <span class="token" class:quality={token.isQuality}>{token.display}</span>
              {/each}
            </div>
          {:else}
            <div class="diff-empty">无</div>
          {/if}
        </div>
        <div class="diff-column">
          <h3 class="diff-head">仅目标有 <span class="diff-count">{diff.onlyRight.length}</span></h3>
          {#if diff.onlyRight.length}
            <div class="token-list">
              {#each diff.onlyRight as token, index (index)}
                <span class="token" class:quality={token.isQuality}>{token.display}</span>
              {/each}
            </div>
          {:else}
            <div class="diff-empty">无</div>
          {/if}
        </div>
      </div>
      <p class="diff-note">官方质量词（如 very aesthetic、masterpiece）淡化显示。</p>
    {/if}
  </section>

  <section class="diff-block">
    <h2 class="block-title">生成参数对照</h2>
    <table class="param-table">
      <thead>
        <tr>
          <th scope="col">参数</th>
          <th scope="col">样本</th>
          <th scope="col">目标</th>
        </tr>
      </thead>
      <tbody>
        {#each paramRows as row (row.label)}
          <tr class:differs={row.different}>
            <th scope="row">{row.label}</th>
            <td class="tabular">{row.sampleValue}</td>
            <td class="tabular">{row.targetValue}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </section>
</div>

<style>
  .side-by-side {
    display: flex;
    flex-direction: column;
    gap: 18px;
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 20px 26px 30px;
  }

  .head {
    flex: none;
  }

  .back {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text-2);
    font-size: var(--font-sm);
    font-weight: 600;
    padding: 6px 12px;
    border-radius: 999px;
    cursor: pointer;
  }

  .back:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .panes {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 18px;
  }

  .pane {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .pane-media {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--surface);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-1);
    overflow: hidden;
    height: 340px;
  }

  .media-note {
    font-size: var(--font-sm);
    color: var(--text-3);
  }

  .version-badge {
    position: absolute;
    top: 8px;
    left: 8px;
    z-index: 1;
  }

  .pane-caption {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .pane-kind {
    flex: none;
    font-size: var(--font-xs);
    font-weight: 700;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    padding: 2px 8px;
    border-radius: 999px;
  }

  .pane-name {
    flex: 1;
    min-width: 0;
    font-size: var(--font-sm);
    font-weight: 600;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .locate {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text-2);
    font-size: var(--font-xs);
    font-weight: 600;
    padding: 4px 9px;
    border-radius: 999px;
    cursor: pointer;
  }

  .locate:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .diff-block {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .block-title {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 700;
    color: var(--text);
  }

  .block-empty {
    font-size: var(--font-sm);
    color: var(--text-3);
    background: var(--surface);
    border-radius: var(--radius-m);
    padding: 18px 16px;
    text-align: center;
  }

  .diff-columns {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 14px;
  }

  .diff-column {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    background: var(--surface);
    border-radius: var(--radius-m);
    padding: 12px;
  }

  .diff-head {
    margin: 0;
    font-size: var(--font-sm);
    font-weight: 700;
    color: var(--text-2);
  }

  .diff-count {
    font-size: var(--font-xs);
    color: var(--text-3);
    font-weight: 600;
  }

  .token-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .token {
    font-size: var(--font-xs);
    color: var(--text);
    background: var(--surface-2);
    border-radius: var(--radius-s);
    padding: 3px 8px;
    word-break: break-all;
    max-width: 100%;
  }

  .token.quality {
    color: var(--text-4);
    background: transparent;
  }

  .diff-empty {
    font-size: var(--font-xs);
    color: var(--text-4);
  }

  .diff-note {
    margin: 0;
    font-size: var(--font-xs);
    color: var(--text-3);
  }

  .param-table {
    border-collapse: collapse;
    width: 100%;
    max-width: 720px;
    background: var(--surface);
    border-radius: var(--radius-m);
    overflow: hidden;
  }

  .param-table th,
  .param-table td {
    text-align: left;
    padding: 8px 14px;
    font-size: var(--font-sm);
  }

  .param-table thead th {
    font-size: var(--font-xs);
    font-weight: 700;
    color: var(--text-3);
    background: var(--surface-2);
  }

  .param-table tbody th {
    font-weight: 600;
    color: var(--text-2);
    white-space: nowrap;
  }

  .param-table tbody td {
    color: var(--text);
    word-break: break-all;
  }

  .param-table tbody tr.differs th,
  .param-table tbody tr.differs td {
    background: color-mix(in srgb, var(--warning, #8a6100) 9%, transparent);
  }

  .param-table tbody tr.differs td {
    font-weight: 700;
  }

  @media (max-width: 720px) {
    .panes,
    .diff-columns {
      grid-template-columns: 1fr;
    }
  }
</style>
