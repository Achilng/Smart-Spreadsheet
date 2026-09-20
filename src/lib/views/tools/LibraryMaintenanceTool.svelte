<script lang="ts">
  import { errorText, formatCount } from "../../utils/format";
  import { runAction } from "../../stores/tasks";
  import { runPhashBackfill } from "../../features/library/maintenance";
  import { setNotice } from "../../stores/notices.svelte";
  import { libraryState } from "../../stores/library-state.svelte";
  import { taskState } from "../../stores/task-state.svelte";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";

  import { openRejectedImagesDirectory } from "../../api";

  async function openRejectedDirectory(): Promise<void> {
    await runAction(async () => {
      await openRejectedImagesDirectory();
      setNotice({ tone: "success", text: "已在文件管理器中打开失败图片目录。" });
    });
  }
</script>

<div class="tool-page">
  <section class="maintenance-card tool-card">
    <div class="card-icon"><RefreshCw size={20} strokeWidth={1.7} /></div>
    <div class="card-copy">
      <h3>刷新感知哈希</h3>
      <p>为缺少或过期的图片重新计算感知哈希。以图搜图依赖这项数据。</p>
      {#if taskState.phashProgress}
        <div class="phash-progress">
          <span class="progress" role="progressbar" aria-valuemin={0} aria-valuemax={taskState.phashProgress.total} aria-valuenow={taskState.phashProgress.processed}>
            <span
              class="progress-fill"
              style:transform="scaleX({taskState.phashProgress.total > 0 ? taskState.phashProgress.processed / taskState.phashProgress.total : 0})"
            ></span>
          </span>
          <span class="progress-text tabular">
            {formatCount(taskState.phashProgress.processed)} / {formatCount(taskState.phashProgress.total)}
          </span>
        </div>
      {/if}
    </div>
    <button
      type="button"
      class="btn btn-primary"
      disabled={taskState.busy}
      onclick={() => void runPhashBackfill()}
    >
      {taskState.phashProgress ? "正在计算…" : "开始刷新"}
    </button>
  </section>

  <section class="maintenance-card tool-card">
    <div class="card-icon"><FolderOpen size={20} strokeWidth={1.7} /></div>
    <div class="card-copy">
      <h3>失败图片目录</h3>
      <p>查看导入时因元数据异常而被移出的图片，便于手动检查和整理。</p>
      {#if libraryState.snapshot?.rejectedImagesDirectory}
        <code title={libraryState.snapshot.rejectedImagesDirectory}>
          {libraryState.snapshot.rejectedImagesDirectory}
        </code>
      {/if}
    </div>
    <button
      type="button"
      class="btn"
      disabled={taskState.busy}
      onclick={() => void openRejectedDirectory()}
    >
      打开目录
    </button>
  </section>

  {#if libraryState.snapshot?.startupError}
    <p class="error-box">{errorText(libraryState.snapshot.startupError)}</p>
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
  }

  .card-icon {
    width: 42px;
    height: 42px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-m);
    background: var(--accent-soft);
    color: var(--accent);
  }

  .phash-progress {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 10px;
    max-width: 320px;
  }

  .progress-text {
    flex: none;
    font-size: var(--font-xs);
    color: var(--text-3);
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
