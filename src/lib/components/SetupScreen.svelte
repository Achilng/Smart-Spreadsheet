<script lang="ts">
  import { app, chooseDirectory } from "../app-state.svelte";
</script>

<div class="center-screen">
  <div class="flow-card">
    <h2>选择一个数据目录开始</h2>
    <p class="muted">数据库、工作簿副本和图片缓存会统一保存在这里。之后更改位置时，应用会完整迁移数据。</p>
    <div class="flow-actions">
      <button
        type="button"
        class="btn btn-primary"
        disabled={app.busy}
        onclick={() => void chooseDirectory("initialize")}
      >
        初始化数据目录
      </button>
      <button
        type="button"
        class="btn"
        disabled={app.busy}
        onclick={() => void chooseDirectory("open")}
      >
        打开已有目录
      </button>
    </div>
    <p class="flow-hint">初始化只接受空文件夹或已由智能表格管理的目录。</p>
  </div>
</div>
