<script lang="ts">
  import { emitTo } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  import {
    applyQuickTag,
    getRowsByIds,
    listTags,
    previewQuickTag,
    reapplyQuickTagChanges,
    revertQuickTagChanges,
    type QuickEditCondition,
    type QuickTagApplyResult,
    type QuickTagPreview,
    type RowRecord,
    type TagSummary,
  } from "../../api";
  import {
    errorText,
    formatCount,
    notifyMainStateChanged,
    setNotice,
  } from "../../stores/app-state.svelte";
  import {
    history,
    recordHistory,
    redoLastAction,
    undoLastAction,
  } from "../../stores/history.svelte";
  import Thumbnail from "../../ui/Thumbnail.svelte";
  import { focusMainWindow, type ToolboxRowRequest } from "../../windows/toolbox";

  let { active = false }: { active?: boolean } = $props();

  let promptText = $state("");
  let tagSearch = $state("");
  let tags = $state<TagSummary[]>([]);
  let selectedTags = $state<string[]>([]);
  let tagsLoading = $state(false);
  let previewing = $state(false);
  let applying = $state(false);
  let preview = $state<QuickTagPreview | null>(null);
  let sampleRows = $state<RowRecord[]>([]);
  let lastResult = $state<QuickTagApplyResult | null>(null);
  let error = $state<string | null>(null);
  let openingRowId = $state<number | null>(null);

  const requiredTokens = $derived(parseRequiredTokens(promptText));
  const visibleTags = $derived(
    tagSearch.trim()
      ? tags.filter(tag => tag.name.toLocaleLowerCase().includes(tagSearch.trim().toLocaleLowerCase()))
      : tags,
  );
  const canPreview = $derived(
    requiredTokens.length > 0 &&
      selectedTags.length > 0 &&
      !previewing &&
      !applying &&
      !history.busy,
  );

  onMount(() => {
    void refreshTags();
  });

  function condition(): QuickEditCondition {
    return {
      fields: ["positivePrompt", "characterPrompt", "negativePrompt"],
      requiredTokens: [...requiredTokens],
    };
  }

  function parseRequiredTokens(value: string): string[] {
    const seen = new Set<string>();
    const tokens: string[] = [];
    for (const part of value.split(/[,\n\r]+/)) {
      const token = part.trim();
      const identity = token.toLocaleLowerCase();
      if (token && !seen.has(identity)) {
        seen.add(identity);
        tokens.push(token);
      }
    }
    return tokens;
  }

  function invalidatePreview(): void {
    preview = null;
    sampleRows = [];
    lastResult = null;
    error = null;
  }

  function updatePromptText(event: Event): void {
    promptText = (event.currentTarget as HTMLTextAreaElement).value;
    invalidatePreview();
  }

  function toggleTag(name: string): void {
    if (selectedTags.includes(name)) {
      selectedTags = selectedTags.filter(tag => tag !== name);
    } else {
      selectedTags = [...selectedTags, name];
    }
    invalidatePreview();
  }

  async function refreshTags(): Promise<void> {
    tagsLoading = true;
    try {
      tags = await listTags();
    } catch (cause) {
      error = `无法读取 Tag 库：${errorText(cause)}`;
    } finally {
      tagsLoading = false;
    }
  }

  async function runPreview(): Promise<void> {
    if (!canPreview) return;
    previewing = true;
    error = null;
    lastResult = null;
    try {
      const result = await previewQuickTag(condition(), selectedTags);
      const rows = result.sampleRowIds.length > 0
        ? await getRowsByIds(result.sampleRowIds)
        : [];
      preview = result;
      sampleRows = rows;
    } catch (cause) {
      preview = null;
      sampleRows = [];
      error = errorText(cause);
    } finally {
      previewing = false;
    }
  }

  async function runApply(): Promise<void> {
    if (!preview || preview.associationsToAdd === 0 || applying || history.busy) return;
    applying = true;
    error = null;
    try {
      const result = await applyQuickTag(condition(), selectedTags);
      lastResult = result;
      if (result.changes.length > 0) {
        const changes = result.changes.map(change => ({ ...change }));
        const label = `快速打 Tag（${formatCount(result.changedRows)} 张）`;
        recordHistory({
          label,
          undo: async () => {
            await revertQuickTagChanges(changes);
            invalidatePreview();
            await refreshAfterMutation();
          },
          redo: async () => {
            await reapplyQuickTagChanges(changes);
            invalidatePreview();
            await refreshAfterMutation();
          },
        });
      }
      preview = {
        ...preview,
        rowsNeedingChanges: 0,
        alreadyTaggedRows: preview.matchedRows,
        associationsToAdd: 0,
      };
      await refreshAfterMutation();
      setNotice({
        tone: "success",
        text: result.associationsChanged > 0
          ? `快速打标完成：${formatCount(result.changedRows)} 张图片新增了 ${formatCount(result.associationsChanged)} 个 Tag 关联。`
          : "所有命中图片已经拥有所选 Tag，没有产生修改。",
      });
    } catch (cause) {
      error = errorText(cause);
    } finally {
      applying = false;
    }
  }

  async function refreshAfterMutation(): Promise<void> {
    await Promise.all([refreshTags(), notifyMainStateChanged("libraryEdited")]);
  }

  async function openInMain(rowId: number): Promise<void> {
    openingRowId = rowId;
    try {
      const request: ToolboxRowRequest = { rowId };
      await emitTo("main", "toolbox://open-row", request);
      await focusMainWindow();
    } catch (cause) {
      setNotice({
        tone: "error",
        text: `无法在主窗口打开图片：${errorText(cause)}`,
      });
    } finally {
      openingRowId = null;
    }
  }

  function rowName(row: RowRecord): string {
    const path = row.imagePath ?? row.storedImagePath;
    return path?.split(/[\\/]/).pop() ?? `图片 #${row.id}`;
  }

  function onKeydown(event: KeyboardEvent): void {
    if (!active || history.busy || applying || previewing) return;
    const target = event.target;
    const isTextEditing =
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      (target instanceof HTMLElement && target.isContentEditable);
    if (!(event.ctrlKey || event.metaKey) || event.altKey || isTextEditing) return;

    const key = event.key.toLocaleLowerCase();
    if (key === "z") {
      event.preventDefault();
      if (event.shiftKey) {
        void redoLastAction();
      } else {
        void undoLastAction();
      }
    } else if (key === "y" && !event.shiftKey) {
      event.preventDefault();
      void redoLastAction();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="quick-edit-page">
  <div class="operation-bar">
    <div class="operation-switcher" aria-label="快速编辑操作类型">
      <button type="button" class="is-active">添加 Tag</button>
      <button type="button" disabled title="后续版本开放">提示词操作</button>
    </div>
    <div class="history-actions">
      <button
        type="button"
        class="btn btn-ghost"
        disabled={history.undoCount === 0 || history.busy || applying || previewing}
        title={history.undoLabel ? `撤回：${history.undoLabel}` : "没有可撤回的快速编辑"}
        onclick={() => void undoLastAction()}
      >
        ↶ 撤回
      </button>
      <button
        type="button"
        class="btn btn-ghost"
        disabled={history.redoCount === 0 || history.busy || applying || previewing}
        title={history.redoLabel ? `重做：${history.redoLabel}` : "没有可重做的快速编辑"}
        onclick={() => void redoLastAction()}
      >
        ↷ 重做
      </button>
    </div>
  </div>

  <div class="editor-layout">
    <div class="rule-column">
      <section class="rule-card">
        <div class="step-heading">
          <span>1</span>
          <div>
            <h3>输入提示词组合</h3>
            <p>组合中的每一项都必须存在，顺序和位置不限。</p>
          </div>
        </div>

        <textarea
          value={promptText}
          rows="4"
          placeholder="例如：genshin, hutao"
          aria-label="必须同时存在的提示词组合"
          oninput={updatePromptText}
        ></textarea>

        {#if requiredTokens.length > 0}
          <div class="token-list" aria-label="已识别的提示词条件">
            {#each requiredTokens as token (token)}
              <span>{token}</span>
            {/each}
          </div>
        {/if}

        <div class="match-rules">
          <span>扫描范围：整个资料库</span>
          <span>严格匹配，仅忽略大小写与 NovelAI 权重语法</span>
        </div>
      </section>

      <section class="rule-card tag-card">
        <div class="step-heading">
          <span>2</span>
          <div>
            <h3>选择要添加的 Tag</h3>
            <p>可以多选；图片原有 Tag 不会被移除。</p>
          </div>
        </div>

        <input
          class="tag-search"
          type="search"
          bind:value={tagSearch}
          placeholder="搜索现有 Tag"
          aria-label="搜索现有 Tag"
        />

        <div class="tag-list" aria-label="现有 Tag 列表">
          {#if tagsLoading}
            <p class="list-state">正在读取 Tag 库…</p>
          {:else if tags.length === 0}
            <p class="list-state">Tag 库为空，请先在主窗口创建 Tag。</p>
          {:else if visibleTags.length === 0}
            <p class="list-state">没有匹配的 Tag。</p>
          {:else}
            {#each visibleTags as tag (tag.name)}
              <button
                type="button"
                class:is-selected={selectedTags.includes(tag.name)}
                aria-pressed={selectedTags.includes(tag.name)}
                onclick={() => toggleTag(tag.name)}
              >
                <span class="check" aria-hidden="true">
                  {selectedTags.includes(tag.name) ? "✓" : ""}
                </span>
                <strong title={tag.name}>{tag.name}</strong>
                <small>{formatCount(tag.rowCount)}</small>
              </button>
            {/each}
          {/if}
        </div>

        {#if selectedTags.length > 0}
          <div class="selected-summary">已选择 {formatCount(selectedTags.length)} 个 Tag</div>
        {/if}
      </section>
    </div>

    <section class="preview-card">
      <div class="preview-heading">
        <div>
          <h3>执行预览</h3>
          <p>先扫描并确认影响范围，再执行修改。</p>
        </div>
        <button
          type="button"
          class="btn"
          disabled={!canPreview}
          onclick={() => void runPreview()}
        >
          {previewing ? "扫描中…" : "预览匹配结果"}
        </button>
      </div>

      {#if error}
        <p class="error-message">{error}</p>
      {:else if preview}
        <div class="metrics">
          <div>
            <strong>{formatCount(preview.scannedRows)}</strong>
            <span>扫描图片</span>
          </div>
          <div>
            <strong>{formatCount(preview.matchedRows)}</strong>
            <span>命中组合</span>
          </div>
          <div class:is-highlight={preview.rowsNeedingChanges > 0}>
            <strong>{formatCount(preview.rowsNeedingChanges)}</strong>
            <span>需要修改</span>
          </div>
          <div>
            <strong>{formatCount(preview.alreadyTaggedRows)}</strong>
            <span>已有全部 Tag</span>
          </div>
        </div>

        {#if sampleRows.length > 0}
          <div class="sample-heading">
            <strong>命中示例</strong>
            <span>最多展示 12 张，点击可在主窗口定位</span>
          </div>
          <div class="sample-grid">
            {#each sampleRows as row (row.id)}
              <button
                type="button"
                title={rowName(row)}
                disabled={openingRowId !== null}
                onclick={() => void openInMain(row.id)}
              >
                <span class="sample-image">
                  <Thumbnail
                    rowId={row.id}
                    hasImage={Boolean(row.imagePath || row.storedImagePath)}
                    alt={rowName(row)}
                  />
                </span>
                <span>{rowName(row)}</span>
              </button>
            {/each}
          </div>
        {:else}
          <div class="empty-preview">
            <strong>没有图片命中这个提示词组合</strong>
            <span>请检查拼写；空格和下划线会被严格区分。</span>
          </div>
        {/if}

        <div class="apply-panel">
          <div>
            {#if preview.associationsToAdd > 0}
              将为 {formatCount(preview.rowsNeedingChanges)} 张图片新增
              {formatCount(preview.associationsToAdd)} 个 Tag 关联
            {:else if preview.matchedRows > 0}
              命中图片已经拥有所选 Tag
            {:else}
              当前规则没有可执行的修改
            {/if}
          </div>
          <button
            type="button"
            class="btn btn-primary"
            disabled={preview.associationsToAdd === 0 || applying || history.busy}
            onclick={() => void runApply()}
          >
            {applying ? "正在应用…" : "执行打标"}
          </button>
        </div>

        {#if lastResult}
          <p class="result-message">
            已修改 {formatCount(lastResult.changedRows)} 张图片，共新增
            {formatCount(lastResult.associationsChanged)} 个 Tag 关联。
          </p>
        {/if}
      {:else}
        <div class="preview-placeholder">
          <span class="preview-icon">⌕</span>
          <strong>等待预览</strong>
          <p>输入提示词组合并选择目标 Tag 后，扫描整个资料库。</p>
        </div>
      {/if}
    </section>
  </div>
</div>

<style>
  .quick-edit-page {
    min-height: 100%;
    padding: 20px 24px 30px;
  }

  .operation-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 14px;
  }

  .operation-switcher {
    display: inline-flex;
    padding: 3px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface-2);
  }

  .operation-switcher button {
    min-width: 92px;
    padding: 6px 12px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--text-3);
    font-size: var(--font-sm);
  }

  .operation-switcher button.is-active {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow-1);
  }

  .history-actions {
    display: flex;
    gap: 2px;
  }

  .history-actions .btn {
    padding-inline: 10px;
    font-size: var(--font-sm);
  }

  .editor-layout {
    display: grid;
    grid-template-columns: minmax(300px, 0.82fr) minmax(340px, 1.18fr);
    gap: 16px;
    align-items: start;
  }

  .rule-column {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
  }

  .rule-card,
  .preview-card {
    min-width: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    background: var(--surface);
    box-shadow: var(--shadow-1);
  }

  .rule-card {
    padding: 18px;
  }

  .step-heading {
    display: flex;
    align-items: flex-start;
    gap: 11px;
    margin-bottom: 14px;
  }

  .step-heading > span {
    width: 25px;
    height: 25px;
    display: grid;
    place-items: center;
    flex: none;
    border-radius: 8px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: var(--font-sm);
    font-weight: 700;
  }

  .step-heading h3,
  .preview-heading h3 {
    font-size: var(--font-lg);
  }

  .step-heading p,
  .preview-heading p {
    margin-top: 2px;
    color: var(--text-3);
    font-size: var(--font-sm);
  }

  textarea,
  .tag-search {
    width: 100%;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    background: var(--surface);
    color: var(--text);
  }

  textarea {
    min-height: 88px;
    resize: vertical;
    padding: 10px 11px;
    line-height: 1.55;
  }

  textarea:focus,
  .tag-search:focus {
    border-color: var(--accent);
    box-shadow: var(--focus-ring);
  }

  .token-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
  }

  .token-list span {
    max-width: 100%;
    overflow: hidden;
    padding: 3px 8px;
    border-radius: var(--radius-full);
    background: var(--accent-soft);
    color: var(--accent);
    font-size: var(--font-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .match-rules {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin-top: 13px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .tag-card {
    padding-bottom: 13px;
  }

  .tag-search {
    height: 35px;
    padding: 0 10px;
  }

  .tag-list {
    max-height: 210px;
    min-height: 72px;
    margin-top: 9px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
  }

  .tag-list button {
    width: 100%;
    min-height: 36px;
    display: grid;
    grid-template-columns: 20px minmax(0, 1fr) auto;
    align-items: center;
    gap: 7px;
    padding: 5px 9px;
    border: 0;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    text-align: left;
  }

  .tag-list button:last-child {
    border-bottom: 0;
  }

  .tag-list button:hover {
    background: var(--surface-2);
  }

  .tag-list button.is-selected {
    background: var(--accent-soft);
    color: var(--accent);
  }

  .tag-list .check {
    width: 17px;
    height: 17px;
    display: grid;
    place-items: center;
    border: 1px solid var(--border-strong);
    border-radius: 5px;
    background: var(--surface);
    font-size: 11px;
    font-weight: 700;
  }

  .tag-list button.is-selected .check {
    border-color: var(--accent);
    background: var(--accent);
    color: white;
  }

  .tag-list strong {
    overflow: hidden;
    font-size: var(--font-sm);
    font-weight: 550;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag-list small {
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .list-state {
    display: grid;
    min-height: 72px;
    place-items: center;
    padding: 12px;
    color: var(--text-3);
    font-size: var(--font-sm);
    text-align: center;
  }

  .selected-summary {
    margin-top: 7px;
    color: var(--accent);
    font-size: var(--font-xs);
  }

  .preview-card {
    min-height: 480px;
    display: flex;
    flex-direction: column;
    padding: 18px;
  }

  .preview-heading {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
  }

  .preview-heading .btn {
    padding-inline: 12px;
    font-size: var(--font-sm);
  }

  .metrics {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
    margin-top: 17px;
  }

  .metrics > div {
    min-width: 0;
    padding: 10px;
    border-radius: var(--radius-s);
    background: var(--surface-2);
  }

  .metrics > div.is-highlight {
    background: var(--accent-soft);
  }

  .metrics strong,
  .metrics span {
    display: block;
  }

  .metrics strong {
    overflow: hidden;
    font-size: var(--font-lg);
    text-overflow: ellipsis;
  }

  .metrics span {
    margin-top: 1px;
    color: var(--text-3);
    font-size: var(--font-xs);
    white-space: nowrap;
  }

  .sample-heading {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
    margin: 17px 1px 8px;
    font-size: var(--font-sm);
  }

  .sample-heading span {
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .sample-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 7px;
  }

  .sample-grid button {
    min-width: 0;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--surface);
    text-align: left;
  }

  .sample-grid button:hover:not(:disabled) {
    border-color: var(--accent);
  }

  .sample-image {
    height: 72px;
    display: block;
    overflow: hidden;
    background: var(--surface-3);
  }

  .sample-grid button > span:last-child {
    display: block;
    overflow: hidden;
    padding: 5px 6px;
    color: var(--text-2);
    font-size: var(--font-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .apply-panel {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    margin-top: auto;
    padding-top: 16px;
    border-top: 1px solid var(--border);
    color: var(--text-2);
    font-size: var(--font-sm);
  }

  .result-message,
  .error-message {
    margin-top: 10px;
    padding: 9px 11px;
    border-radius: var(--radius-s);
    font-size: var(--font-sm);
  }

  .result-message {
    background: var(--success-soft);
    color: var(--success);
  }

  .error-message {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .preview-placeholder,
  .empty-preview {
    flex: 1;
    min-height: 230px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--text-3);
    text-align: center;
  }

  .preview-placeholder strong,
  .empty-preview strong {
    color: var(--text-2);
    font-size: var(--font-md);
  }

  .preview-placeholder p,
  .empty-preview span {
    max-width: 300px;
    margin-top: 3px;
    font-size: var(--font-sm);
  }

  .preview-icon {
    width: 40px;
    height: 40px;
    display: grid;
    place-items: center;
    margin-bottom: 8px;
    border-radius: 12px;
    background: var(--surface-3);
    color: var(--text-3);
    font-size: 20px;
  }

  @media (max-width: 840px) {
    .quick-edit-page {
      padding: 16px;
    }

    .editor-layout {
      grid-template-columns: 1fr;
    }

    .preview-card {
      min-height: 430px;
    }
  }

  @media (max-width: 680px) {
    .operation-bar {
      align-items: flex-start;
      flex-direction: column;
    }

    .metrics {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
