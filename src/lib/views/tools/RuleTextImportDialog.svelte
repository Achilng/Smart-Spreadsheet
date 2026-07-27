<script lang="ts">
  import ClipboardPaste from "@lucide/svelte/icons/clipboard-paste";
  import FileSearch from "@lucide/svelte/icons/file-search";
  import { onMount } from "svelte";

  import Modal from "../../ui/Modal.svelte";

  const MAX_TEXT_BYTES = 2 * 1024 * 1024;

  let {
    value,
    busy,
    error,
    onchange,
    onclose,
    oninspect,
  }: {
    value: string;
    busy: boolean;
    error: string | null;
    onchange: (value: string) => void;
    onclose: () => void;
    oninspect: () => void;
  } = $props();

  let textarea = $state<HTMLTextAreaElement | null>(null);
  const byteCount = $derived(new TextEncoder().encode(value).length);
  const tooLarge = $derived(byteCount > MAX_TEXT_BYTES);
  const canInspect = $derived(value.trim().length > 0 && !tooLarge && !busy);

  onMount(() => {
    requestAnimationFrame(() => textarea?.focus());
  });
</script>

<Modal open={true} {onclose} {busy} labelledby="rule-text-import-title" width="680px">
  <header class="dialog-head">
    <div class="title-icon"><ClipboardPaste size={21} aria-hidden="true" /></div>
    <div>
      <h2 id="rule-text-import-title">粘贴 JSON 文本</h2>
      <p>可粘贴纯 JSON，或 AI 回复中的一个 json 代码块。</p>
    </div>
  </header>

  <div class="dialog-body">
    <label for="rule-json-text">规则 JSON</label>
    <textarea
      id="rule-json-text"
      bind:this={textarea}
      value={value}
      disabled={busy}
      spellcheck="false"
      placeholder={'粘贴 { "format": "smart-spreadsheet-automation-rules", ... }\n\n也支持粘贴含有 ```json 代码块的 AI 回复。'}
      oninput={event => onchange(event.currentTarget.value)}
    ></textarea>
    <div class="text-meta">
      <span>这里只检查内容，不会立即写入资料库。</span>
      <span class:invalid={tooLarge}>{(byteCount / 1024).toFixed(byteCount >= 1024 ? 1 : 0)} KB / 2048 KB</span>
    </div>
    {#if tooLarge}
      <p class="inline-error" role="alert">粘贴内容超过 2 MB 上限，请减少规则数量后重试。</p>
    {:else if error}
      <p class="inline-error" role="alert">{error}</p>
    {/if}
  </div>

  <footer class="dialog-actions">
    <button type="button" class="btn" disabled={busy} onclick={onclose}>取消</button>
    <button type="button" class="btn btn-primary" disabled={!canInspect} onclick={oninspect}>
      <FileSearch size={15} aria-hidden="true" />{busy ? "检查中…" : "检查并预览"}
    </button>
  </footer>
</Modal>

<style>
  .dialog-head { display: flex; align-items: center; gap: 11px; padding: 18px 20px 14px; border-bottom: 1px solid var(--border); }
  .title-icon { width: 38px; height: 38px; display: grid; place-items: center; flex: none; border-radius: 10px; background: var(--accent-soft); color: var(--accent); }
  .dialog-head h2 { font-size: var(--font-lg); }
  .dialog-head p { margin-top: 2px; color: var(--text-3); font-size: var(--font-xs); }
  .dialog-body { display: grid; gap: 7px; padding: 16px 20px; }
  label { color: var(--text-2); font-size: var(--font-sm); font-weight: 600; }
  textarea { width: 100%; min-height: 300px; resize: vertical; padding: 11px 12px; border: 1px solid var(--border); border-radius: var(--radius-m); background: var(--surface-2); color: var(--text); font: 12.5px/1.55 var(--font-mono); tab-size: 2; }
  textarea:focus { border-color: var(--accent); outline: 2px solid var(--accent-soft); }
  textarea:disabled { opacity: .65; }
  .text-meta { display: flex; justify-content: space-between; gap: 12px; color: var(--text-3); font-size: var(--font-xs); }
  .text-meta .invalid, .inline-error { color: var(--danger); }
  .inline-error { padding: 8px 10px; border-radius: var(--radius-s); background: var(--danger-soft); font-size: var(--font-sm); }
  .dialog-actions { display: flex; justify-content: flex-end; gap: 8px; padding: 12px 20px 16px; border-top: 1px solid var(--border); }
</style>
