<script lang="ts">
  import { app, setNotice } from "../app-state.svelte";
</script>

{#if app.notice}
  <div class="toast toast-{app.notice.tone}" role="status">
    <span>{app.notice.text}</span>
    <button type="button" aria-label="关闭提示" onclick={() => setNotice(null)}>×</button>
  </div>
{/if}

<style>
  .toast {
    position: fixed;
    bottom: 20px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 100;
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: min(640px, calc(100vw - 48px));
    padding: 10px 14px;
    border-radius: var(--radius-s);
    border: 1px solid;
    background: var(--surface);
    box-shadow: var(--shadow-2);
    font-size: 13px;
  }

  .toast-success {
    border-color: var(--success);
    color: var(--success);
    background: var(--success-soft);
  }

  .toast-error {
    border-color: var(--danger);
    color: var(--danger);
    background: var(--danger-soft);
  }

  .toast span {
    overflow-wrap: anywhere;
  }

  .toast button {
    border: none;
    background: none;
    color: inherit;
    font-size: 16px;
    line-height: 1;
    padding: 0 2px;
  }
</style>
