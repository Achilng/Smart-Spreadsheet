<script lang="ts">
  import Modal from "../../ui/Modal.svelte";
  import { cancelDropImport, confirmDropImport, dropState } from "../../stores/drop-import.svelte";

  function displayName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }
</script>

<Modal open={dropState.open} onclose={cancelDropImport} busy={dropState.busy} labelledby="drop-title" width="480px">
  <div class="drop-dialog">
    <header>
      <h2 id="drop-title">
        确定要导入以下 {dropState.paths.length} 个项目吗？
      </h2>
      <p>将追加导入到当前资料库。</p>
      {#if dropState.ignoredCount > 0}
        <p class="ignored-hint">
          另有 {dropState.ignoredCount} 个不支持的文件已被忽略（只支持 PNG、文件夹和 zip / 7z / rar）。
        </p>
      {/if}
    </header>

    <ul class="path-list">
      {#each dropState.paths as path (path)}
        <li title={path}>{displayName(path)}</li>
      {/each}
    </ul>

    <footer>
      <button
        type="button"
        class="btn"
        disabled={dropState.busy}
        onclick={cancelDropImport}
      >
        取消
      </button>
      <button
        type="button"
        class="btn btn-primary"
        disabled={dropState.busy}
        onclick={() => void confirmDropImport()}
      >
        {dropState.busy
          ? dropState.paths.length > 1
            ? `正在导入第 ${dropState.currentIndex}/${dropState.paths.length} 个…`
            : "导入中…"
          : "确认导入"}
      </button>
    </footer>
  </div>
</Modal>

<style>
  .drop-dialog {
    padding: 18px;
  }

  header h2 {
    font-size: var(--font-lg);
    margin-bottom: 6px;
  }

  header p {
    color: var(--text-2);
    font-size: var(--font-md);
    line-height: 1.55;
  }

  .ignored-hint {
    margin-top: 4px;
    color: var(--danger);
    font-size: var(--font-sm);
  }

  .path-list {
    margin: 14px 0;
    padding: 10px 14px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    list-style: none;
    max-height: 200px;
    overflow-y: auto;
  }

  .path-list li {
    font-size: var(--font-md);
    line-height: 1.7;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text);
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
