<script lang="ts">
  import { app, closeSearchResults, formatCount } from "../../stores/app-state.svelte";
  import { getRowThumbnail, getRowsByIds, type RowRecord } from "../../api";
  import { showContextMenu } from "../../stores/context-menu.svelte";
  import { beginFileDrag } from "../../stores/file-drag";
  import { rowStore } from "../../stores/row-store.svelte";

  const results = $derived(app.searchResults ?? []);
  const queryName = $derived(app.searchQueryPath?.split(/[\\/]/).pop() ?? "");

  let thumbnailUrls = $state<Map<number, string>>(new Map());
  let rowRecords = $state<Map<number, RowRecord>>(new Map());

  $effect(() => {
    const current = results;
    if (current.length === 0) {
      rowRecords = new Map();
      return;
    }
    const ids = current.slice(0, 50).map((m) => m.rowId);
    getRowsByIds(ids)
      .then((rows) => {
        const map = new Map<number, RowRecord>();
        for (const row of rows) map.set(row.id, row);
        rowRecords = map;
      })
      .catch(() => {});
  });

  $effect(() => {
    const current = results;
    for (const match of current.slice(0, 50)) {
      if (!thumbnailUrls.has(match.rowId)) {
        getRowThumbnail(match.rowId)
          .then((buffer: ArrayBuffer) => {
            const blob = new Blob([buffer], { type: "image/png" });
            const url = URL.createObjectURL(blob);
            thumbnailUrls = new Map([...thumbnailUrls, [match.rowId, url]]);
          })
          .catch(() => {});
      }
    }
  });

  function handleClose() {
    for (const url of thumbnailUrls.values()) URL.revokeObjectURL(url);
    thumbnailUrls = new Map();
    closeSearchResults();
  }

  function onContextMenu(event: MouseEvent, rowId: number): void {
    const row = rowRecords.get(rowId);
    if (!row) return;
    event.preventDefault();
    rowStore.activeRow = row;
    showContextMenu(row, event.clientX, event.clientY);
  }

  function distanceLabel(distance: number): string {
    if (distance === 0) return "完全匹配";
    if (distance <= 3) return "极高";
    if (distance <= 6) return "高";
    if (distance <= 10) return "中等";
    return "低";
  }
</script>

{#if results.length > 0}
  <div class="overlay" role="dialog" aria-label="搜索结果">
    <div class="panel">
      <header class="panel-header">
        <h2>以图搜图结果</h2>
        <span class="query-name" title={app.searchQueryPath ?? ""}>
          查询图片：{queryName}
        </span>
        <span class="result-count">
          找到 {formatCount(results.length)} 张相似图片
        </span>
        <button type="button" class="close-btn" onclick={handleClose}>关闭</button>
      </header>

      <div class="results-grid">
        {#each results as match (match.rowId)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="result-card" oncontextmenu={(e) => onContextMenu(e, match.rowId)}>
            <div class="card-thumb">
              {#if thumbnailUrls.has(match.rowId)}
                <img
                  src={thumbnailUrls.get(match.rowId)}
                  alt="行 {match.rowId}"
                  loading="lazy"
                  draggable="false"
                  onmousedown={(e) => beginFileDrag(e, match.rowId)}
                />
              {:else}
                <div class="placeholder">加载中</div>
              {/if}
            </div>
            <div class="card-info">
              <span class="distance" class:exact={match.distance === 0}>
                相似度：{distanceLabel(match.distance)}
              </span>
              <span class="distance-value">行 {match.rowId} · 距离 {match.distance}</span>
            </div>
          </div>
        {/each}
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .panel {
    background: var(--surface);
    border-radius: var(--radius-l, 12px);
    box-shadow: var(--shadow-3, 0 8px 32px rgba(0, 0, 0, 0.2));
    max-width: 900px;
    max-height: 80vh;
    width: 90vw;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .panel-header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }

  .panel-header h2 {
    font-size: 16px;
    font-weight: 600;
    margin: 0;
  }

  .query-name {
    font-size: 13px;
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 200px;
  }

  .result-count {
    font-size: 13px;
    color: var(--text-2);
    margin-left: auto;
  }

  .close-btn {
    padding: 4px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-s, 6px);
    background: var(--surface);
    font-size: 13px;
    cursor: pointer;
    flex: none;
  }

  .close-btn:hover {
    background: var(--surface-2);
  }

  .results-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 12px;
    padding: 16px 20px;
    overflow-y: auto;
  }

  .result-card {
    border: 1px solid var(--border);
    border-radius: var(--radius-s, 6px);
    overflow: hidden;
    background: var(--bg);
    cursor: pointer;
    padding: 0;
    text-align: left;
    display: flex;
    flex-direction: column;
  }

  .result-card:hover {
    border-color: var(--accent, #4a90d9);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
  }

  .card-thumb {
    aspect-ratio: 1;
    background: var(--bg);
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }

  .card-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .placeholder {
    color: var(--text-3);
    font-size: 12px;
  }

  .card-info {
    padding: 6px 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .distance {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
  }

  .distance.exact {
    color: var(--success, #34a853);
  }

  .distance-value {
    font-size: 11px;
    color: var(--text-3);
  }
</style>
