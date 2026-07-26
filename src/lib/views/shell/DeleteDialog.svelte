<script lang="ts">
  import Modal from "../../ui/Modal.svelte";
  import { formatCount } from "../../stores/app-state.svelte";
  import { cancelDelete, confirmDelete, deletion } from "../../stores/delete-actions.svelte";
</script>

<Modal open={deletion.open} onclose={cancelDelete} busy={deletion.busy} labelledby="delete-title" width="440px">
  <div class="delete-dialog">
    <header>
      <h2 id="delete-title">删除 {formatCount(deletion.count)} 行？</h2>
      <p class="irreversible-warning">
        此操作不会加入撤销记录，删除后无法通过 Ctrl+Z 恢复，并会清空当前撤销/重做记录。
      </p>
      <p>对应的 Tag 关联、分组关系、受管图片副本和缩略图缓存也会永久清理。</p>
    </header>

    <label class="trash-option">
      <input type="checkbox" bind:checked={deletion.trashOriginals} disabled={deletion.busy} />
      <span>
        <strong>同时将原始图片文件移入回收站</strong>
        <small>默认勾选。原图移入 Windows 回收站后，应用内无法恢复；压缩包来源没有独立原文件，会自动跳过。</small>
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
</Modal>

<style>
  .delete-dialog {
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

  header .irreversible-warning {
    margin-bottom: 4px;
    color: var(--danger);
    font-weight: 600;
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
    font-size: var(--font-md);
  }

  .trash-option small {
    color: var(--text-2);
    font-size: var(--font-sm);
    line-height: 1.5;
  }

  .dialog-error {
    margin-bottom: 12px;
    color: var(--danger);
    font-size: var(--font-sm);
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
