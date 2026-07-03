<script lang="ts">
  import { findReplacePrompt, prefixArtistTag, type RowSelection } from "../../api";
  import Modal from "../../ui/Modal.svelte";
  import { errorText } from "../../stores/app-state.svelte";
  import { resetRows } from "../../stores/row-store.svelte";

  interface Props {
    selection: RowSelection;
    count: number;
    onclose: () => void;
  }

  let { selection, count, onclose }: Props = $props();

  let tab = $state<"replace" | "artist">("replace");
  let findText = $state("");
  let replaceText = $state("");
  let artistName = $state("");
  let busy = $state(false);
  let result = $state<string | null>(null);
  let error = $state<string | null>(null);

  async function doReplace(): Promise<void> {
    if (busy || !findText.trim()) return;
    busy = true;
    error = null;
    result = null;
    try {
      const r = await findReplacePrompt(selection, findText, replaceText);
      result = `已替换 ${r.affectedRows} 行`;
      resetRows();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function doPrepend(): Promise<void> {
    if (busy || !artistName.trim()) return;
    busy = true;
    error = null;
    result = null;
    try {
      const r = await prefixArtistTag(selection, artistName.trim());
      result = `已修正 ${r.affectedRows} 行画师前缀`;
      resetRows();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal open={true} {onclose} width="420px">
  <div class="dialog-content">
    <header>
      <h3>编辑提示词（{count} 行）</h3>
      <button type="button" class="close-btn" onclick={onclose}>&times;</button>
    </header>

    <nav class="tabs">
      <button type="button" class:active={tab === "replace"} onclick={() => (tab = "replace")}>查找替换</button>
      <button type="button" class:active={tab === "artist"} onclick={() => (tab = "artist")}>修正画师前缀</button>
    </nav>

    <div class="body">
      {#if tab === "replace"}
        <label>
          <span>查找</span>
          <input type="text" bind:value={findText} placeholder="要替换的文本…" disabled={busy} />
        </label>
        <label>
          <span>替换为</span>
          <input type="text" bind:value={replaceText} placeholder="替换后的文本（可为空）…" disabled={busy} />
        </label>
        <button
          type="button"
          class="btn btn-primary action-btn"
          disabled={busy || !findText.trim()}
          onclick={() => void doReplace()}
        >
          {busy ? "执行中…" : "执行替换"}
        </button>
      {:else}
        <label>
          <span>画师名（不带 artist:）</span>
          <input
            type="text"
            bind:value={artistName}
            placeholder="例如 parsley_f"
            disabled={busy}
            onkeydown={e => { if (e.key === "Enter") void doPrepend(); }}
          />
        </label>
        <p class="hint">
          只修正选中图片中完全匹配的该画师 tag；已带 artist: 的会跳过，支持 (tag:1.2)、花/方括号权重、0.7::tag 等格式。
        </p>
        <button
          type="button"
          class="btn btn-primary action-btn"
          disabled={busy || !artistName.trim()}
          onclick={() => void doPrepend()}
        >
          {busy ? "执行中…" : "修正前缀"}
        </button>
      {/if}

      {#if result}
        <p class="result-ok">{result}</p>
      {/if}
      {#if error}
        <p class="result-err">{error}</p>
      {/if}
    </div>
  </div>
</Modal>

<style>
  .dialog-content {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }

  header h3 {
    font-size: var(--font-base);
    font-weight: 600;
  }

  .close-btn {
    border: none;
    background: none;
    font-size: var(--font-xl);
    color: var(--text-2);
    padding: 0 4px;
    cursor: pointer;
  }

  .tabs {
    display: flex;
    border-bottom: 1px solid var(--border);
  }

  .tabs button {
    flex: 1;
    border: none;
    background: none;
    padding: 8px;
    font-size: var(--font-md);
    color: var(--text-2);
    cursor: pointer;
    border-bottom: 2px solid transparent;
  }

  .tabs button.active {
    color: var(--accent);
    border-bottom-color: var(--accent);
    font-weight: 600;
  }

  .body {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  label span {
    font-size: var(--font-sm);
    color: var(--text-2);
    font-weight: 600;
  }

  label input {
    padding: 6px 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    background: var(--surface);
    font-size: var(--font-md);
  }

  label input:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: var(--focus-ring);
  }

  .hint {
    margin: -2px 0 0;
    font-size: var(--font-sm);
    line-height: 1.5;
    color: var(--text-2);
  }

  .action-btn {
    align-self: flex-end;
    padding: 6px 16px;
    font-size: var(--font-md);
    background: var(--accent);
    color: white;
    border: 1px solid var(--accent);
    border-radius: var(--radius-s);
    cursor: pointer;
  }

  .action-btn:hover:not(:disabled) {
    filter: brightness(1.1);
  }

  .action-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .result-ok {
    font-size: var(--font-md);
    color: var(--success, #22c55e);
  }

  .result-err {
    font-size: var(--font-md);
    color: var(--danger);
  }
</style>
