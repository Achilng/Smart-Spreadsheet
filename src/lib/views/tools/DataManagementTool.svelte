<script lang="ts">
  import {
    app,
    chooseMigration,
    resetDataWithConfirmation,
  } from "../../stores/app-state.svelte";
</script>

<div class="tool-page">
  <section class="data-card">
    <div class="card-copy">
      <span class="eyebrow">数据目录</span>
      <h3>迁移资料库</h3>
      <p>将数据库、受管图片和缓存复制并校验到一个新的空文件夹，成功后自动切换。</p>
      {#if app.snapshot?.dataDirectory}
        <code title={app.snapshot.dataDirectory}>{app.snapshot.dataDirectory}</code>
      {/if}
    </div>
    <button
      type="button"
      class="btn"
      disabled={app.busy}
      onclick={() => void chooseMigration()}
    >
      选择迁移目标…
    </button>
  </section>

  <section class="data-card danger-card">
    <div class="card-copy">
      <span class="eyebrow danger">危险操作</span>
      <h3>重置表格</h3>
      <p>清空数据库、受管图片和缩略图，回到初始导入页面。外部原始图片不会被删除。</p>
    </div>
    <button
      type="button"
      class="btn btn-danger"
      disabled={app.busy}
      onclick={() => void resetDataWithConfirmation()}
    >
      重置表格…
    </button>
  </section>
</div>

<style>
  .tool-page {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-height: 100%;
    padding: 24px 28px 32px;
  }

  .data-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 28px;
    padding: 22px;
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    background: var(--surface);
    box-shadow: var(--shadow-1);
  }

  .danger-card {
    border-color: var(--danger-border-soft);
  }

  .card-copy {
    min-width: 0;
  }

  .eyebrow {
    display: block;
    margin-bottom: 4px;
    color: var(--accent);
    font-size: var(--font-xs);
    font-weight: 700;
    letter-spacing: 0.08em;
  }

  .eyebrow.danger {
    color: var(--danger);
  }

  h3 {
    font-size: var(--font-lg);
  }

  p {
    margin-top: 4px;
    color: var(--text-2);
    font-size: var(--font-md);
  }

  code {
    display: block;
    overflow: hidden;
    margin-top: 10px;
    color: var(--text-3);
    font-family: var(--font);
    font-size: var(--font-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
