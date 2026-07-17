<script lang="ts">
  import { emitTo } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";

  import {
    getRowsByIds,
    searchSimilarImages,
    type RowRecord,
    type SimilarImageMatch,
  } from "../../api";
  import { errorText, formatCount, setNotice } from "../../stores/app-state.svelte";
  import Thumbnail from "../../ui/Thumbnail.svelte";
  import {
    focusMainWindow,
    type ToolboxRowAction,
    type ToolboxRowRequest,
  } from "../../windows/toolbox";

  let queryPath = $state<string | null>(null);
  let matches = $state<SimilarImageMatch[]>([]);
  let rows = $state<Map<number, RowRecord>>(new Map());
  let searching = $state(false);
  let searched = $state(false);
  let error = $state<string | null>(null);
  let openingRowId = $state<number | null>(null);

  const queryName = $derived(queryPath?.split(/[\\/]/).pop() ?? null);

  async function chooseAndSearch(): Promise<void> {
    const selection = await open({
      multiple: false,
      directory: false,
      title: "选择用于搜索的图片",
      filters: [
        {
          name: "图片",
          extensions: ["png", "jpg", "jpeg", "bmp", "gif", "webp", "tiff"],
        },
      ],
    });
    if (typeof selection !== "string") return;

    queryPath = selection;
    matches = [];
    rows = new Map();
    searched = false;
    searching = true;
    error = null;
    try {
      const result = await searchSimilarImages(selection, 10);
      const records = result.length > 0
        ? await getRowsByIds(result.map(match => match.rowId))
        : [];
      matches = result;
      rows = new Map(records.map(row => [row.id, row]));
      searched = true;
    } catch (cause) {
      error = errorText(cause);
    } finally {
      searching = false;
    }
  }

  function distanceLabel(distance: number): string {
    if (distance === 0) return "完全匹配";
    if (distance <= 3) return "极高相似";
    if (distance <= 6) return "高度相似";
    return "可能相似";
  }

  function rowName(rowId: number): string {
    const row = rows.get(rowId);
    const path = row?.imagePath ?? row?.storedImagePath;
    return path?.split(/[\\/]/).pop() ?? `图片 #${rowId}`;
  }

  async function openInMain(rowId: number, action: ToolboxRowAction): Promise<void> {
    openingRowId = rowId;
    try {
      const request: ToolboxRowRequest = { rowId, action };
      await emitTo("main", "toolbox://open-row", request);
      await focusMainWindow();
    } catch (cause) {
      setNotice({
        tone: "error",
        text: `无法在主窗口打开图片：${errorText(cause)}`,
      });
    } finally {
      openingRowId = null;
    }
  }
</script>

<div class="tool-page">
  <section class="intro-card">
    <div>
      <h3>选择一张参考图片</h3>
      <p>工具会计算参考图的感知哈希，并返回距离不超过 10 的库内图片。</p>
    </div>
    <button
      type="button"
      class="btn btn-primary"
      disabled={searching}
      onclick={() => void chooseAndSearch()}
    >
      {searching ? "搜索中…" : queryPath ? "换一张图片" : "选择图片…"}
    </button>
  </section>

  {#if queryName}
    <div class="query-row">
      <span>参考图片</span>
      <strong title={queryPath}>{queryName}</strong>
    </div>
  {/if}

  {#if error}
    <p class="message error">{error}</p>
  {:else if searched && matches.length === 0}
    <div class="empty-result">
      <strong>没有找到相似图片</strong>
      <span>可以先到“资料库维护”刷新感知哈希后再试。</span>
    </div>
  {:else if matches.length > 0}
    <div class="result-heading">
      <strong>搜索结果</strong>
      <span>{formatCount(matches.length)} 张</span>
    </div>
    <div class="results-grid">
      {#each matches as match (match.rowId)}
        <article class="result-card">
          <button
            type="button"
            class="thumbnail"
            title="在主窗口查看详情"
            disabled={openingRowId !== null}
            onclick={() => void openInMain(match.rowId, "detail")}
          >
            <Thumbnail rowId={match.rowId} hasImage={true} alt={rowName(match.rowId)} />
          </button>
          <div class="card-info">
            <strong title={rowName(match.rowId)}>{rowName(match.rowId)}</strong>
            <span class:exact={match.distance === 0}>
              {distanceLabel(match.distance)} · 距离 {match.distance}
            </span>
          </div>
          <div class="card-actions">
            <button
              type="button"
              disabled={openingRowId !== null}
              onclick={() => void openInMain(match.rowId, "detail")}
            >
              查看详情
            </button>
            <button
              type="button"
              disabled={openingRowId !== null}
              onclick={() => void openInMain(match.rowId, "table")}
            >
              表格定位
            </button>
          </div>
        </article>
      {/each}
    </div>
  {/if}
</div>

<style>
  .tool-page {
    min-height: 100%;
    padding: 24px 28px 32px;
  }

  .intro-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    padding: 20px;
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    background: var(--surface);
    box-shadow: var(--shadow-1);
  }

  .intro-card h3 {
    font-size: var(--font-lg);
  }

  .intro-card p {
    margin-top: 4px;
    color: var(--text-2);
    font-size: var(--font-md);
  }

  .query-row {
    display: flex;
    gap: 12px;
    min-width: 0;
    padding: 16px 2px 12px;
    color: var(--text-3);
    font-size: var(--font-sm);
  }

  .query-row strong {
    overflow: hidden;
    color: var(--text-2);
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .message {
    margin-top: 18px;
    padding: 12px 14px;
    border-radius: var(--radius-s);
    font-size: var(--font-md);
  }

  .message.error {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .empty-result {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    margin-top: 40px;
    color: var(--text-3);
    font-size: var(--font-md);
  }

  .empty-result strong {
    color: var(--text-2);
    font-size: var(--font-lg);
  }

  .result-heading {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 10px 2px 12px;
  }

  .result-heading span {
    color: var(--text-3);
    font-size: var(--font-sm);
  }

  .results-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 14px;
  }

  .result-card {
    min-width: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    background: var(--surface);
    box-shadow: var(--shadow-1);
  }

  .thumbnail {
    display: block;
    width: 100%;
    padding: 0;
    aspect-ratio: 1;
    overflow: hidden;
    border: 0;
    background: var(--surface-2);
    cursor: pointer;
  }

  .thumbnail:disabled {
    cursor: wait;
  }

  .thumbnail :global(.thumbnail-stack),
  .thumbnail :global(img) {
    width: 100%;
    height: 100%;
  }

  .thumbnail :global(img) {
    object-fit: cover;
  }

  .card-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 9px 10px 10px;
  }

  .card-info strong {
    overflow: hidden;
    font-size: var(--font-sm);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .card-info span {
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .card-info span.exact {
    color: var(--success);
  }

  .card-actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    border-top: 1px solid var(--border);
  }

  .card-actions button {
    min-width: 0;
    padding: 8px 5px;
    border: 0;
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: var(--font-xs);
    cursor: pointer;
  }

  .card-actions button + button {
    border-left: 1px solid var(--border);
  }

  .card-actions button:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--accent);
  }

  .card-actions button:disabled {
    cursor: wait;
    opacity: 0.55;
  }
</style>
