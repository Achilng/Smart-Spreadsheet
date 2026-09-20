<script lang="ts">
  import type { Snippet } from "svelte";
  import Modal from "./Modal.svelte";

  let { open, title, warning, description, busy = false, error = null, oncancel, onconfirm, children }: {
    open: boolean; title: string; warning: string; description: string;
    busy?: boolean; error?: string | null; oncancel: () => void; onconfirm: () => void; children?: Snippet;
  } = $props();
  const id = $props.id();
</script>

<Modal {open} onclose={oncancel} {busy} labelledby={`${id}-title`} width="440px">
  <div class="delete-dialog">
    <header>
      <h2 id={`${id}-title`}>{title}</h2>
      <p class="irreversible-warning">{warning}</p>
      <p>{description}</p>
    </header>
    {@render children?.()}
    {#if error}<p class="dialog-error" role="alert">{error}</p>{/if}
    <footer>
      <button type="button" class="btn" disabled={busy} onclick={oncancel}>取消</button>
      <button type="button" class="btn btn-danger" disabled={busy} onclick={onconfirm}>{busy ? "正在删除…" : "确认删除"}</button>
    </footer>
  </div>
</Modal>

<style>
  .delete-dialog { padding: 18px; overflow-y: auto; }
  header h2 { font-size: var(--font-lg); margin-bottom: 6px; overflow-wrap: anywhere; }
  header p { color: var(--text-2); font-size: var(--font-md); line-height: 1.55; }
  header .irreversible-warning { margin-bottom: 4px; color: var(--danger); font-weight: 600; }
  .dialog-error { margin-top: 12px; color: var(--danger); font-size: var(--font-sm); }
  footer { margin-top: 16px; display: flex; justify-content: flex-end; gap: 8px; }
</style>
