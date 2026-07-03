<script lang="ts">
  import { cancelDropImport, confirmDropImport, dropState } from "../../stores/drop-import.svelte";

  function displayName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }
</script>

<svelte:window
  onkeydown={(event) => {
    if (event.key === "Escape" && dropState.open) {
      event.preventDefault();
      cancelDropImport();
    }
  }}
/>

{#if dropState.open}
  <div
    class="dialog-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) {
        cancelDropImport();
      }
    }}
  >
    <div
      class="drop-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="drop-title"
      tabindex="-1"
    >
      <header>
        <h2 id="drop-title">
          确定要导入以下 {dropState.paths.length} 个项目吗？
        </h2>
        <p>将追加导入到当前资料库。</p>
      </header>

      <ul class="path-list">
        {#each dropState.paths as path}
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
          {dropState.busy ? "导入中…" : "确认导入"}
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .dialog-backdrop {
    position: fixed;
    inset: 0;
    z-index: 110;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgb(15 20 28 / 55%);
  }

  .drop-dialog {
    width: min(480px, 100%);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-2);
    padding: 18px;
  }

  header h2 {
    font-size: 17px;
    margin-bottom: 6px;
  }

  header p {
    color: var(--text-2);
    font-size: 13px;
    line-height: 1.55;
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
    font-size: 13px;
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
