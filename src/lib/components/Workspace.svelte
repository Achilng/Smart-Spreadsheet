<script lang="ts">
  import { app } from "../app-state.svelte";
  import { loadTags } from "../tag-store.svelte";
  import DetailPanel from "./DetailPanel.svelte";
  import GalleryView from "./GalleryView.svelte";
  import TopBar from "./TopBar.svelte";

  // 首次挂载和工作簿替换时刷新 Tag 库
  $effect(() => {
    void app.dataVersion;
    void loadTags();
  });
</script>

<div class="workspace">
  <TopBar />
  <div class="workspace-body">
    <aside class="sidebar">
      <div class="panel-placeholder">
        <h3>Tag 库</h3>
        <p class="faint">筛选与新建 Tag（阶段 4 接入）</p>
      </div>
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
