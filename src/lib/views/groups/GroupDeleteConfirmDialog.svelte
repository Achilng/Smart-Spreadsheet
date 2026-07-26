<script lang="ts">
  import Modal from "../../ui/Modal.svelte";
  import { formatCount } from "../../stores/app-state.svelte";
  import {
    cancelGroupDelete,
    confirmGroupDelete,
    groupDeleteConfirm,
  } from "../../stores/group-delete-confirm.svelte";

  const group = $derived(groupDeleteConfirm.group);
</script>

<Modal open={groupDeleteConfirm.open && group !== null} onclose={cancelGroupDelete} width="420px">
  {#if group}
    <div class="dialog">
      <header>
        <h2>删除分组「{group.name}」？</h2>
        <p>
          组内 {formatCount(group.memberCount)} 张图片<strong>不会被删除</strong>，将回到“未分组”。
        </p>
        <p class="hint">删除后可用 Ctrl+Z 撤销。</p>
      </header>
      <footer>
        <button
          type="button"
          class="btn"
          disabled={groupDeleteConfirm.busy}
          onclick={cancelGroupDelete}
        >取消</button>
        <button
          type="button"
          class="btn btn-danger"
          disabled={groupDeleteConfirm.busy}
          onclick={() => void confirmGroupDelete()}
        >
          {groupDeleteConfirm.busy ? "正在删除…" : "删除分组"}
        </button>
      </footer>
    </div>
  {/if}
</Modal>

<style>
  .dialog {
    padding: 18px;
  }

  header h2 {
    font-size: var(--font-lg);
    margin-bottom: 8px;
  }

  header p {
    color: var(--text-2);
    font-size: var(--font-md);
    line-height: 1.55;
  }

  header p strong {
    color: var(--text);
  }

  .hint {
    margin-top: 4px;
    font-size: var(--font-sm);
    color: var(--text-3);
  }

  footer {
    margin-top: 16px;
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
