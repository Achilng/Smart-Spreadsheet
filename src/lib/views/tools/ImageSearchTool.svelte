<script lang="ts">
  import { emitTo } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { open } from "@tauri-apps/plugin-dialog";
  import ImageUp from "@lucide/svelte/icons/image-up";
  import { onMount } from "svelte";

  import {
    getRowsByIds,
    searchSimilarImages,
    type RowRecord,
    type SimilarImageMatch,
  } from "../../api";
  import { errorText, formatCount, setNotice } from "../../stores/app-state.svelte";
  import Thumbnail from "../../ui/Thumbnail.svelte";
  import { focusMainWindow, type ToolboxRowRequest } from "../../windows/toolbox";
  import { softFade, softFly } from "../../ui/motion";

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  let queryPath = $state<string | null>(null);
  let matches = $state<SimilarImageMatch[]>([]);
  let rows = $state<Map<number, RowRecord>>(new Map());
  let searching = $state(false);
  let searched = $state(false);
  let error = $state<string | null>(null);
  let openingRowId = $state<number | null>(null);
  let draggingOverSearch = $state(false);
  let searchDropZone: HTMLElement;

  const IMAGE_EXTENSIONS = new Set([
    "png",
    "jpg",
    "jpeg",
    "bmp",
    "gif",
    "webp",
    "tif",
    "tiff",
  ]);

  const queryName = $derived(queryPath?.split(/[\\/]/).pop() ?? null);

  onMount(() => {
    let disposed = false;
    let unlistenDragDrop: (() => void) | null = null;
    void getCurrentWebview().onDragDropEvent(event => {
      if (!active || searching) {
        draggingOverSearch = false;
        return;
      }
      if (event.payload.type === "enter" || event.payload.type === "over") {
        draggingOverSearch = isInsideSearchDropZone(event.payload.position);
      } else if (event.payload.type === "leave") {
        draggingOverSearch = false;
      } else {
        const shouldSearch = isInsideSearchDropZone(event.payload.position);
        draggingOverSearch = false;
        if (shouldSearch) {
          void searchDroppedPaths(event.payload.paths);
        }
      }
    }).then(unlisten => {
      if (disposed) unlisten();
      else unlistenDragDrop = unlisten;
    });

    return () => {
      disposed = true;
      unlistenDragDrop?.();
    };
  });

  async function chooseAndSearch(): Promise<void> {
    const selection = await open({
      multiple: false,
      directory: false,
      title: "选择用于搜索的图片",
      filters: [
        {
          name: "图片",
          extensions: ["png", "jpg", "jpeg", "bmp", "gif", "webp", "tif", "tiff"],
        },
      ],
    });
    if (typeof selection !== "string") return;

    await runSearch(selection);
  }

  async function runSearch(path: string): Promise<void> {
    queryPath = path;
    matches = [];
    rows = new Map();
    searched = false;
    searching = true;
    error = null;
    try {
      const result = await searchSimilarImages(path, 10);
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

  async function searchDroppedPaths(paths: string[]): Promise<void> {
    if (paths.length !== 1 || !isSupportedImagePath(paths[0])) {
      error = paths.length > 1
        ? "请一次只拖入一张参考图片。"
        : "请拖入支持的图片文件（PNG、JPG、BMP、GIF、WebP 或 TIFF）。";
      return;
    }
    await runSearch(paths[0]);
  }

  function isSupportedImagePath(path: string): boolean {
    const extension = path.split(".").pop()?.toLowerCase();
    return extension !== undefined && IMAGE_EXTENSIONS.has(extension);
  }

  function isInsideSearchDropZone(position: { x: number; y: number }): boolean {
    if (!searchDropZone) return false;
    const scale = window.devicePixelRatio || 1;
    const x = position.x / scale;
    const y = position.y / scale;
    const rect = searchDropZone.getBoundingClientRect();
    return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
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

  async function openInMain(rowId: number): Promise<void> {
    openingRowId = rowId;
    try {
      const request: ToolboxRowRequest = { rowId };
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
  <section
    bind:this={searchDropZone}
    class="intro-card tool-card"
    class:is-dragging={draggingOverSearch}
  >
    <div class="drop-copy">
      <span class="drop-icon" aria-hidden="true"><ImageUp size={24} strokeWidth={1.6} /></span>
      <div>
        <h3>{draggingOverSearch ? "松开即可开始搜索" : "选择或拖入一张参考图片"}</h3>
        <p>工具会计算参考图的感知哈希，并返回距离不超过 10 的库内图片。</p>
        <small>支持 PNG、JPG、BMP、GIF、WebP 和 TIFF</small>
      </div>
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
    <div class="query-row" transition:softFly={{ duration: 150, y: 4 }}>
      <span>参考图片</span>
      <strong title={queryPath}>{queryName}</strong>
    </div>
  {/if}

  {#if searching}
    <div class="results-grid skeleton-grid" aria-label="正在搜索相似图片" transition:softFade={{ duration: 120 }}>
      {#each Array(6) as _}
        <div class="result-card skeleton-card" aria-hidden="true">
          <span class="thumbnail shimmer"></span>
          <span class="card-info">
            <span class="skeleton-line"></span>
            <span class="skeleton-line short"></span>
          </span>
        </div>
      {/each}
    </div>
  {:else if error}
    <p class="message error" role="alert" transition:softFly={{ duration: 150, y: 4 }}>{error}</p>
  {:else if searched && matches.length === 0}
    <div class="empty-result empty-state" transition:softFly={{ duration: 160, y: 4 }}>
      <strong>没有找到相似图片</strong>
      <span>可以先到“资料库维护”刷新感知哈希后再试。</span>
    </div>
  {:else if matches.length > 0}
    <div class="result-heading" transition:softFade={{ duration: 140 }}>
      <strong>搜索结果</strong>
      <span>{formatCount(matches.length)} 张</span>
    </div>
    <div class="results-grid">
      {#each matches as match, index (match.rowId)}
        <button
          type="button"
          class="result-card"
          title="在主窗口画廊中定位"
          disabled={openingRowId !== null}
          class:is-opening={openingRowId === match.rowId}
          onclick={() => void openInMain(match.rowId)}
          in:softFly={{ duration: 165, y: 4, delay: Math.min(index, 7) * 15 }}
        >
          <span class="thumbnail">
            <Thumbnail rowId={match.rowId} hasImage={true} alt={rowName(match.rowId)} />
          </span>
          <span class="card-info">
            <strong title={rowName(match.rowId)}>{rowName(match.rowId)}</strong>
            <span class:exact={match.distance === 0}>
              {distanceLabel(match.distance)} · 距离 {match.distance}
            </span>
          </span>
          {#if openingRowId === match.rowId}
            <span class="opening-indicator spinner" aria-label="正在主窗口中打开"></span>
          {/if}
        </button>
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
    border-style: dashed;
    transition:
      border-color var(--motion-fast) var(--ease-responsive),
      background var(--motion-fast) var(--ease-responsive),
      transform var(--motion-fast) var(--ease-responsive);
  }

  .intro-card.is-dragging {
    border-color: var(--accent);
    background: var(--accent-soft);
    transform: translateY(-1px);
  }

  .drop-copy {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .drop-icon {
    width: 44px;
    height: 44px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: var(--radius-s);
    background: var(--accent-soft);
    color: var(--accent);
  }

  .intro-card h3 {
    font-size: var(--font-lg);
  }

  .intro-card p {
    margin-top: 4px;
    color: var(--text-2);
    font-size: var(--font-md);
  }

  .intro-card small {
    display: block;
    margin-top: 5px;
    color: var(--text-3);
    font-size: var(--font-xs);
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
    margin-top: 40px;
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

  .skeleton-grid {
    margin-top: 20px;
  }

  .result-card {
    padding: 0;
    min-width: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    background: var(--surface);
    color: inherit;
    font: inherit;
    text-align: left;
    box-shadow: var(--shadow-1);
    cursor: pointer;
    position: relative;
    transition:
      border-color var(--motion-fast) var(--ease-responsive),
      box-shadow var(--motion-fast) var(--ease-responsive),
      transform var(--motion-fast) var(--ease-responsive);
  }

  .result-card:hover:not(:disabled) {
    border-color: var(--accent);
    box-shadow: var(--shadow-hover);
    transform: translateY(-2px);
  }

  .result-card:active:not(:disabled) {
    transform: translateY(-1px) scale(0.985);
    transition-duration: var(--motion-press);
  }

  .result-card:disabled {
    cursor: wait;
  }

  .result-card:disabled:not(.is-opening) {
    opacity: 0.78;
  }

  .skeleton-card {
    pointer-events: none;
  }

  .skeleton-line {
    display: block;
    width: 78%;
    height: 10px;
    border-radius: var(--radius-full);
    background: var(--surface-2);
  }

  .skeleton-line.short {
    width: 52%;
  }

  .opening-indicator {
    position: absolute;
    inset: 8px 8px auto auto;
    width: 16px;
    height: 16px;
    color: var(--accent);
  }

  .thumbnail {
    display: block;
    width: 100%;
    aspect-ratio: 1;
    overflow: hidden;
    background: var(--surface-2);
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

  @media (max-width: 680px) {
    .intro-card {
      align-items: stretch;
      flex-direction: column;
    }

    .intro-card > .btn {
      align-self: flex-end;
    }
  }
</style>
