<script lang="ts">
  import {
    app,
    chooseImageArchive,
    chooseImageFolder,
    chooseWorkbook,
  } from "../app-state.svelte";
</script>

<div class="center-screen">
  <div class="flow-card">
    <h2>导入第一批数据</h2>
    <p class="muted">
      资料库为追加式：可多次导入，已入库的图片自动跳过。支持 NovelAI PNG
      文件夹、zip/7z/rar 压缩包，以及固定 NovelAI Metadata 结构的 xlsx。原始文件始终只读。
    </p>
    <p class="directory" title={app.snapshot?.dataDirectory}>
      数据目录：{app.snapshot?.dataDirectory}
    </p>
    <div class="flow-actions">
      <button
        type="button"
        class="btn btn-primary"
        disabled={app.busy}
        onclick={() => void chooseImageFolder()}
      >
        导入图片文件夹
      </button>
      <button
        type="button"
        class="btn"
        disabled={app.busy}
        onclick={() => void chooseImageArchive()}
      >
        导入压缩包
      </button>
      <button
        type="button"
        class="btn"
        disabled={app.busy}
        onclick={() => void chooseWorkbook()}
      >
        导入 xlsx
      </button>
    </div>
  </div>
</div>

<style>
  .directory {
    font-size: 12px;
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
