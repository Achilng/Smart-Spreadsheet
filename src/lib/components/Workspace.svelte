<script lang="ts">
  import { app } from "../app-state.svelte";
  import { rowStore } from "../row-store.svelte";
  import GalleryView from "./GalleryView.svelte";
  import TopBar from "./TopBar.svelte";
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

    <aside class="detail">
      <div class="panel-placeholder">
        <h3>详情</h3>
        {#if rowStore.activeRow}
          <p class="faint">已选中 Excel 第 {rowStore.activeRow.sourceRow} 行（阶段 3 接入）</p>
        {:else}
          <p class="faint">点击图片查看（阶段 3 接入）</p>
        {/if}
      </div>
    </aside>
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
