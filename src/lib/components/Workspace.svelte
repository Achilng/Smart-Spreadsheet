<script lang="ts">
  import { app } from "../app-state.svelte";
  import { clearSelection, selectAllFiltered } from "../selection-store.svelte";
  import { rowStore } from "../row-store.svelte";
  import { loadTags } from "../tag-store.svelte";
  import DetailPanel from "./DetailPanel.svelte";
  import GalleryView from "./GalleryView.svelte";
  import SelectionBar from "./SelectionBar.svelte";
  import TagSidebar from "./TagSidebar.svelte";
  import TopBar from "./TopBar.svelte";

  // 首次挂载和工作簿替换时刷新 Tag 库并清空选择
  $effect(() => {
    void app.dataVersion;
    clearSelection();
    void loadTags();
  });

  function onKeydown(event: KeyboardEvent): void {
    // Ctrl+A 全选筛选结果（输入框内除外）
    if (
      event.key.toLowerCase() === "a" &&
      (event.ctrlKey || event.metaKey) &&
      !(event.target instanceof HTMLInputElement) &&
      !(event.target instanceof HTMLTextAreaElement)
    ) {
      event.preventDefault();
      if (rowStore.totalCount > 0) {
        void selectAllFiltered();
      }
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="workspace">
  <TopBar />
  <div class="workspace-body">
    <aside class="sidebar">
      <TagSidebar />
    </aside>

    <main class="main-area">
      {#if app.viewMode === "gallery"}
        <GalleryView />
      {:else}
        <div class="panel-placeholder">
          <h3>表格视图</h3>
          <p class="faint">多列表格（阶段 5 接入）</p>
        </div>
      {/if}
      <SelectionBar />
    </main>

    {#if app.detailOpen}
      <aside class="detail">
        <DetailPanel />
      </aside>
    {:else}
      <button
        type="button"
        class="detail-strip"
        title="展开详情面板"
        onclick={() => (app.detailOpen = true)}
      >
        «
      </button>
    {/if}
  </div>
</div>

<style>
  .workspace {
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  .workspace-body {
    flex: 1;
    display: flex;
    min-height: 0;
  }

  .sidebar {
    width: 240px;
    flex: none;
    background: var(--surface);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .main-area {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    position: relative;
  }

  .detail {
    width: 340px;
    flex: none;
    background: var(--surface);
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .detail-strip {
    width: 22px;
    flex: none;
    border: none;
    border-left: 1px solid var(--border);
    background: var(--surface);
    color: var(--text-3);
    font-size: 13px;
  }

  .detail-strip:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .panel-placeholder {
    margin: auto;
    text-align: center;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .panel-placeholder h3 {
    font-size: 14px;
    color: var(--text-2);
    font-weight: 600;
  }

  .panel-placeholder p {
    font-size: 12px;
  }
</style>
