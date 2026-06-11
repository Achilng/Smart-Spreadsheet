<script lang="ts">
  import { app, chooseWorkbook } from "../app-state.svelte";
</script>

<div class="center-screen">
  <div class="flow-card">
    <h2>导入第一批数据</h2>
    <p class="muted">
      资料库为追加式：可多次导入，已入库的图片自动跳过。当前支持固定 NovelAI Metadata 结构的
      xlsx，原 Excel 始终只读；文件夹与压缩包导入即将提供。
    </p>
    <p class="directory" title={app.snapshot?.dataDirectory}>
      数据目录：{app.snapshot?.dataDirectory}
    </p>
    <div class="flow-actions">
      <button
        type="button"
        class="btn btn-primary"
        disabled={app.busy}
        onclick={() => void chooseWorkbook()}
      >
        选择 Excel
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
