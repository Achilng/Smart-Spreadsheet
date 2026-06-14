<script lang="ts">
  import { app } from "../app-state.svelte";
  import {
    chooseImageArchive,
    chooseImageFolder,
    chooseRejectedImagesDirectory,
  } from "../import-actions.svelte";

  const rejectedDir = $derived(app.snapshot?.rejectedImagesDirectory);
</script>

<div class="center-screen">
  <div class="flow-card">
    <h2>导入第一批数据</h2>
    <p class="muted">
      资料库为追加式：可多次导入，已入库的图片自动跳过。支持 NovelAI PNG
      文件夹、zip/7z/rar 压缩包，以及固定 NovelAI Metadata 结构的 xlsx。无 metadata
      图片不会入库，并会移动到你选择的异常图片目录。
    </p>
    <p class="directory" title={app.snapshot?.dataDirectory}>
      数据目录：{app.snapshot?.dataDirectory}
    </p>
    {#if rejectedDir}
      <p class="directory" title={rejectedDir}>
        异常图片目录：{rejectedDir}
        <button
          type="button"
          class="link-btn"
          disabled={app.busy}
          onclick={() => void chooseRejectedImagesDirectory()}
        >更改</button>
      </p>
    {/if}
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

  .link-btn {
    background: none;
    border: none;
    color: var(--accent);
    cursor: pointer;
    font-size: 12px;
    padding: 0;
    text-decoration: underline;
  }

  .link-btn:hover {
    color: var(--accent-hover, var(--accent));
  }

  .link-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
