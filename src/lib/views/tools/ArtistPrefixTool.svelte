<script lang="ts">
  import { emitTo } from "@tauri-apps/api/event";

  import {
    applyAutoArtistPrefix,
    prefixConfirmedArtistsInText,
    previewAutoArtistPrefix,
    reapplyQuickArtistPrefixChanges,
    revertQuickArtistPrefixChanges,
    type AutoArtistCandidate,
    type AutoArtistPrefixPreview,
    type ArtistTextPrefixResult,
  } from "../../api";
  import {
    app,
    errorText,
    formatCount,
    notifyMainStateChanged,
    setNotice,
  } from "../../stores/app-state.svelte";
  import { history, recordHistory } from "../../stores/history.svelte";
  import { focusMainWindow, type ToolboxRowRequest } from "../../windows/toolbox";

  let preview = $state<AutoArtistPrefixPreview | null>(null);
  let selectedNames = $state<string[]>([]);
  let search = $state("");
  let previewing = $state(false);
  let applying = $state(false);
  let openingRowId = $state<number | null>(null);
  let textInput = $state("");
  let textResult = $state<ArtistTextPrefixResult | null>(null);
  let processingText = $state(false);
  let copyingText = $state(false);
  let error = $state<string | null>(null);

  const filteredCandidates = $derived(
    (preview?.candidates ?? []).filter(candidate => {
      const query = search.trim().toLocaleLowerCase();
      return !query || candidate.matchName.toLocaleLowerCase().includes(query);
    }),
  );
  const selectedCount = $derived(selectedNames.length);
  const busy = $derived(
    previewing || applying || processingText || history.busy || app.busy,
  );

  function updateTextInput(event: Event): void {
    textInput = (event.target as HTMLTextAreaElement).value;
    textResult = null;
  }

  async function processText(): Promise<void> {
    if (!textInput.trim() || busy) return;
    processingText = true;
    error = null;
    try {
      textResult = await prefixConfirmedArtistsInText(textInput);
    } catch (cause) {
      textResult = null;
      error = errorText(cause);
    } finally {
      processingText = false;
    }
  }

  async function copyTextResult(): Promise<void> {
    if (!textResult || copyingText) return;
    copyingText = true;
    try {
      await navigator.clipboard.writeText(textResult.text);
      setNotice({ tone: "success", text: "处理后的纯文本已复制到剪贴板。" });
    } catch (cause) {
      setNotice({ tone: "error", text: `复制失败：${errorText(cause)}` });
    } finally {
      copyingText = false;
    }
  }

  async function scanLibrary(): Promise<void> {
    if (busy) return;
    previewing = true;
    error = null;
    try {
      preview = await previewAutoArtistPrefix();
      selectedNames = preview.candidates.map(candidate => candidate.matchName);
    } catch (cause) {
      preview = null;
      selectedNames = [];
      error = errorText(cause);
    } finally {
      previewing = false;
    }
  }

  async function applySelected(): Promise<void> {
    if (selectedNames.length === 0 || busy) return;
    applying = true;
    error = null;
    try {
      const result = await applyAutoArtistPrefix(selectedNames);
      if (result.changes.length > 0) {
        const changes = result.changes.map(change => ({ ...change }));
        recordHistory({
          label: `自动补全画师前缀（${formatCount(result.changedRows)} 张）`,
          undo: async () => {
            await revertQuickArtistPrefixChanges(changes);
            preview = null;
            selectedNames = [];
            await notifyMainStateChanged("libraryEdited");
          },
          redo: async () => {
            await reapplyQuickArtistPrefixChanges(changes);
            preview = null;
            selectedNames = [];
            await notifyMainStateChanged("libraryEdited");
          },
        });
      }
      await notifyMainStateChanged("libraryEdited");
      setNotice({
        tone: "success",
        text: result.changedRows > 0
          ? `自动修正完成：${formatCount(result.changedRows)} 张图片的 ${formatCount(result.promptFieldsChanged)} 个提示词字段已更新。`
          : "所选候选没有产生修改。",
      });
      await scanLibraryAfterApply();
    } catch (cause) {
      error = errorText(cause);
    } finally {
      applying = false;
    }
  }

  async function scanLibraryAfterApply(): Promise<void> {
    try {
      preview = await previewAutoArtistPrefix();
      selectedNames = preview.candidates.map(candidate => candidate.matchName);
    } catch {
      preview = null;
      selectedNames = [];
    }
  }

  function toggleCandidate(name: string): void {
    selectedNames = selectedNames.includes(name)
      ? selectedNames.filter(selected => selected !== name)
      : [...selectedNames, name];
  }

  function selectAll(): void {
    selectedNames = (preview?.candidates ?? []).map(candidate => candidate.matchName);
  }

  async function openSample(candidate: AutoArtistCandidate): Promise<void> {
    const rowId = candidate.sampleRowIds[0];
    if (!rowId || openingRowId !== null) return;
    openingRowId = rowId;
    try {
      const request: ToolboxRowRequest = { rowId };
      await emitTo("main", "toolbox://open-row", request);
      await focusMainWindow();
    } catch (cause) {
      setNotice({ tone: "error", text: `无法在主窗口打开图片：${errorText(cause)}` });
    } finally {
      openingRowId = null;
    }
  }
</script>

<div class="artist-prefix-page">
  <section class="intro-card tool-card">
    <div>
      <span class="eyebrow overline">LIBRARY ARTIST EVIDENCE</span>
      <h3>补全库内裸画师 Tag</h3>
      <p>
        如果资料库中某处已有 <code>artist:xy</code>，就把其它提示词中的裸 <code>xy</code>
        视为同一画师并补上前缀。没有库内明确证据的名称不会进入候选。
      </p>
    </div>
  </section>

  <section class="text-card tool-card">
    <div class="text-card-heading">
      <div>
        <h3>处理纯文本</h3>
        <p>粘贴提示词后，只为资料库中已有明确标注的同名裸画师 Tag 补上前缀，不会修改资料库。</p>
      </div>
      <button
        class="btn btn-primary"
        type="button"
        disabled={!textInput.trim() || busy}
        onclick={() => void processText()}
      >
        {processingText ? "处理中…" : "添加 artist: 前缀"}
      </button>
    </div>

    <div class="text-workspace" class:has-result={textResult !== null}>
      <label class="text-field">
        <span class="overline">原始文本</span>
        <textarea
          rows="8"
          placeholder="在这里粘贴 NovelAI 提示词…"
          value={textInput}
          oninput={updateTextInput}
        ></textarea>
      </label>

      {#if textResult}
        <label class="text-field result-field">
          <span class="overline">处理结果</span>
          <textarea rows="8" readonly value={textResult.text}></textarea>
        </label>
      {/if}
    </div>

    {#if textResult}
      <div class="text-result-bar">
        <span>
          {textResult.matchedArtists.length > 0
            ? `已识别 ${formatCount(textResult.matchedArtists.length)} 个库内画师，并保留原始格式。`
            : "没有发现需要补前缀的库内画师，原文保持不变。"}
        </span>
        <button
          type="button"
          class="btn"
          disabled={copyingText}
          onclick={() => void copyTextResult()}
        >{copyingText ? "复制中…" : "复制结果"}</button>
      </div>
    {/if}
  </section>

  <section class="scan-card tool-card">
    <div>
      <h3>扫描资料库</h3>
      <p>只处理已经入库的提示词，不联网，也不重新读取或改写原始图片文件。</p>
    </div>
    <button class="btn btn-primary" type="button" disabled={busy} onclick={() => void scanLibrary()}>
      {previewing ? "扫描中…" : "扫描裸画师 Tag"}
    </button>
  </section>

  {#if error}
    <p class="error-message">{error}</p>
  {/if}

  {#if preview}
    <section class="result-card tool-card">
      <div class="metric-grid metrics">
        <div><strong>{formatCount(preview.scannedRows)}</strong><span>扫描图片</span></div>
        <div><strong>{formatCount(preview.matchedRows)}</strong><span>命中图片</span></div>
        <div><strong>{formatCount(preview.candidates.length)}</strong><span>确认画师</span></div>
        <div><strong>{formatCount(preview.promptFieldsNeedingChanges)}</strong><span>提示词字段</span></div>
      </div>

      {#if preview.candidates.length > 0}
        <div class="candidate-toolbar">
          <input type="search" bind:value={search} placeholder="搜索画师名称" />
          <div>
            <button type="button" class="text-button" onclick={selectAll}>全选</button>
            <button type="button" class="text-button" onclick={() => (selectedNames = [])}>清空</button>
          </div>
        </div>

        <div class="candidate-list">
          {#each filteredCandidates as candidate (candidate.matchName)}
            <div class="candidate-row">
              <label>
                <input
                  type="checkbox"
                  checked={selectedNames.includes(candidate.matchName)}
                  onchange={() => toggleCandidate(candidate.matchName)}
                />
              </label>
              <div class="candidate-main">
                <strong>{candidate.displayName}</strong>
                <span class="badge">库内已有明确 artist: 标注</span>
              </div>
              <div class="candidate-stats">
                <strong>{formatCount(candidate.matchedRows)} 张</strong>
                <span>{formatCount(candidate.matchedFields)} 个字段</span>
              </div>
              <button
                type="button"
                class="sample-button"
                disabled={candidate.sampleRowIds.length === 0 || openingRowId !== null}
                onclick={() => void openSample(candidate)}
              >查看示例</button>
            </div>
          {/each}
        </div>

        <div class="apply-bar">
          <div>
            <strong>已选择 {formatCount(selectedCount)} 个画师</strong>
            <span>候选均由资料库内已有的明确画师标注确认。</span>
          </div>
          <button
            class="btn btn-primary"
            type="button"
            disabled={selectedCount === 0 || busy}
            onclick={() => void applySelected()}
          >
            {applying ? "正在应用…" : "为所选画师补全前缀"}
          </button>
        </div>
      {:else}
        <div class="empty-state">
          <strong>没有发现能够由库内证据确认的裸画师 Tag</strong>
          <span>只有同时存在明确 <code>artist:</code> 标注的同名 Tag 才会被识别。</span>
        </div>
      {/if}
    </section>
  {/if}
</div>

<style>
  .artist-prefix-page {
    display: grid;
    gap: 14px;
    padding-bottom: 24px;
  }

  .intro-card,
  .scan-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    padding: 22px;
  }

  .text-card {
    display: grid;
    gap: 16px;
    padding: 20px 22px;
  }

  .text-card-heading,
  .text-result-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
  }

  .text-card-heading > div {
    min-width: 0;
  }

  .text-card-heading .btn {
    flex: 0 0 auto;
  }

  .text-workspace {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 14px;
  }

  .text-workspace.has-result {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .text-field {
    min-width: 0;
    display: grid;
    gap: 7px;
  }

  .text-field > span {
    color: var(--text-3);
  }

  .text-field textarea {
    width: 100%;
    min-height: 150px;
    box-sizing: border-box;
    resize: vertical;
    font-family: inherit;
    line-height: 1.55;
  }

  .result-field textarea {
    background: var(--surface-2);
  }

  .text-result-bar {
    padding-top: 14px;
    border-top: 1px solid var(--border);
  }

  .text-result-bar span {
    color: var(--text-2);
    font-size: var(--font-sm);
  }

  h3,
  p {
    margin: 0;
  }

  h3 {
    color: var(--text);
    font-size: var(--font-lg);
  }

  p {
    margin-top: 7px;
    color: var(--text-2);
    line-height: 1.65;
  }

  code {
    padding: 1px 5px;
    border-radius: var(--radius-s);
    background: var(--surface-2);
    color: var(--accent);
  }

  .eyebrow {
    display: block;
    margin-bottom: 7px;
    color: var(--accent);
  }

  .scan-card p {
    color: var(--text-3);
    font-size: var(--font-sm);
  }

  .error-message {
    padding: 11px 13px;
    border: 1px solid color-mix(in srgb, var(--danger) 35%, var(--border));
    border-radius: var(--radius-m);
    background: color-mix(in srgb, var(--danger) 7%, var(--surface));
    color: var(--danger);
  }

  .result-card {
    overflow: hidden;
  }

  .metrics {
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
  }

  .candidate-toolbar {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
  }

  .candidate-toolbar input {
    width: min(320px, 45%);
  }

  .candidate-toolbar > div {
    display: flex;
    gap: 5px;
  }

  .text-button,
  .sample-button {
    border: 0;
    background: transparent;
    padding: 4px 8px;
    border-radius: var(--radius-full);
    color: var(--accent);
    font-size: 12.5px;
    cursor: pointer;
  }

  .text-button:hover:not(:disabled),
  .sample-button:hover:not(:disabled) {
    background: var(--surface-2);
  }

  .candidate-list {
    max-height: 460px;
    overflow: auto;
  }

  .candidate-row {
    display: grid;
    grid-template-columns: 28px minmax(210px, 1fr) minmax(130px, auto) 78px;
    align-items: center;
    gap: 12px;
    min-height: 70px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border);
  }

  .candidate-row label {
    display: grid;
    place-items: center;
  }

  .candidate-row input[type="checkbox"] {
    width: 16px;
    height: 16px;
  }

  .candidate-main {
    min-width: 0;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
  }

  .candidate-main strong {
    overflow: hidden;
    color: var(--text);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge {
    padding: 2px 6px;
    border-radius: var(--radius-full);
    background: var(--success-soft);
    color: var(--success);
    font-size: 10px;
    font-weight: 600;
  }

  .candidate-stats {
    display: grid;
    justify-items: end;
    gap: 3px;
  }

  .candidate-stats strong {
    color: var(--text);
  }

  .candidate-stats span {
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .sample-button {
    padding: 5px;
    font-size: var(--font-xs);
  }

  .apply-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    padding: 16px;
    background: var(--surface-2);
  }

  .apply-bar > div {
    display: grid;
    gap: 4px;
  }

  .apply-bar strong {
    color: var(--text);
  }

  .apply-bar span {
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .empty-state {
    padding: 54px 20px;
  }

  .empty-state strong {
    color: var(--text-2);
  }

  @media (max-width: 820px) {
    .text-card-heading,
    .text-result-bar {
      align-items: stretch;
      flex-direction: column;
    }

    .text-card-heading .btn,
    .text-result-bar .btn {
      align-self: flex-end;
    }

    .text-workspace.has-result {
      grid-template-columns: minmax(0, 1fr);
    }

    .candidate-row {
      grid-template-columns: 28px minmax(0, 1fr) 90px;
    }

    .sample-button {
      display: none;
    }
  }
</style>
