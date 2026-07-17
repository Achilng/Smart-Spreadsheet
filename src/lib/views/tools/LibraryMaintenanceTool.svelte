<script lang="ts">
  import { openRejectedImagesDirectory } from "../../api";
  import {
    app,
    errorText,
    runAction,
    runPhashBackfill,
    setNotice,
  } from "../../stores/app-state.svelte";

  async function openRejectedDirectory(): Promise<void> {
    await runAction(async () => {
      await openRejectedImagesDirectory();
      setNotice({ tone: "success", text: "已在文件管理器中打开失败图片目录。" });
    });
  }
</script>

<div class="tool-page">
  <section class="maintenance-card">
    <div class="card-icon">↻</div>
    <div class="card-copy">
      <h3>刷新感知哈希</h3>
      <p>为缺少或过期的图片重新计算感知哈希。以图搜图依赖这项数据。</p>
    </div>
    <button
      type="button"
      class="btn btn-primary"
      disabled={app.busy}
      onclick={() => void runPhashBackfill()}
    >
      {app.phashProgress ? "正在计算…" : "开始刷新"}
    </button>
  </section>

  <section class="maintenance-card">
    <div class="card-icon">↗</div>
    <div class="card-copy">
      <h3>失败图片目录</h3>
      <p>查看导入时因元数据异常而被移出的图片，便于手动检查和整理。</p>
      {#if app.snapshot?.rejectedImagesDirectory}
        <code title={app.snapshot.rejectedImagesDirectory}>
          {app.snapshot.rejectedImagesDirectory}
        </code>
      {/if}
    </div>
    <button
      type="button"
      class="btn"
      disabled={app.busy}
      onclick={() => void openRejectedDirectory()}
    >
      打开目录
    </button>
  </section>

  {#if app.snapshot?.startupError}
    <p class="error-box">{errorText(app.snapshot.startupError)}</p>
  {/if}
</div>

<style>
  .tool-page {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 100%;
    padding: 24px 28px 32px;
  }

  .maintenance-card {
    display: grid;
    grid-template-columns: 42px minmax(0, 1fr) auto;
    align-items: center;
    gap: 16px;
    padding: 20px;
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    background: var(--surface);
    box-shadow: var(--shadow-1);
  }

  .card-icon {
    width: 42px;
    height: 42px;
    display: grid;
    place-items: center;
    border-radius: 12px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: var(--font-xl);
    font-weight: 600;
  }

  .card-copy {
    min-width: 0;
  }

  .card-copy h3 {
    font-size: var(--font-lg);
  }

  .card-copy p {
    margin-top: 3px;
    color: var(--text-2);
    font-size: var(--font-md);
  }

  code {
    display: block;
    overflow: hidden;
    margin-top: 7px;
    color: var(--text-3);
    font-family: var(--font);
    font-size: var(--font-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .error-box {
    padding: 12px 14px;
    border-radius: var(--radius-s);
    background: var(--danger-soft);
    color: var(--danger);
    font-size: var(--font-md);
  }
</style>
