<script lang="ts">
  import { formatCount } from "../app-state.svelte";
  import { cancelDelete, confirmDelete, deletion } from "../delete-actions.svelte";
</script>

<svelte:window
  onkeydown={event => {
    if (event.key === "Escape" && deletion.open) {
      event.preventDefault();
      cancelDelete();
    }
  }}
/>

{#if deletion.open}
  <div
    class="dialog-backdrop"
    role="presentation"
    onclick={event => {
      if (event.target === event.currentTarget) {
        cancelDelete();
      }
    }}
  >
    <div
      class="delete-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="delete-title"
      tabindex="-1"
    >
      <header>
        <h2 id="delete-title">删除 {formatCount(deletion.count)} 行？</h2>
        <p>对应的 Tag 关联、受管图片副本和缩略图缓存也会清理。此操作不可撤销。</p>
      </header>

      <label class="trash-option">
        <input type="checkbox" bind:checked={deletion.trashOriginals} disabled={deletion.busy} />
        <span>
          <strong>同时将原始图片文件移入回收站</strong>
          <small>默认不勾选。仅处理文件夹/单 PNG 来源；压缩包来源没有独立原文件，会自动跳过。</small>
        </span>
      </label>

      {#if deletion.error}
        <p class="dialog-error" role="alert">{deletion.error}</p>
      {/if}

      <footer>
        <button type="button" class="btn" disabled={deletion.busy} onclick={cancelDelete}>取消</button>
        <button
          type="button"
          class="btn btn-danger"
          disabled={deletion.busy}
          onclick={() => void confirmDelete()}
        >
          {deletion.busy ? "正在删除…" : "确认删除"}
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

  .delete-dialog {
    width: min(440px, 100%);
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

  .trash-option {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    margin: 16px 0;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--surface-2);
    cursor: pointer;
  }

  .trash-option input {
    margin: 2px 0 0;
    accent-color: var(--danger);
  }

  .trash-option span {
    display: grid;
    gap: 4px;
  }

  .trash-option strong {
    font-size: 13px;
  }

  .trash-option small {
    color: var(--text-2);
    font-size: 12px;
    line-height: 1.5;
  }

  .dialog-error {
    margin-bottom: 12px;
    color: var(--danger);
    font-size: 12.5px;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
