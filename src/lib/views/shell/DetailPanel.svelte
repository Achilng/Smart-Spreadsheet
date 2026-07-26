<script lang="ts">
  import {
    createTag,
    setTagsForRow,
    updateCharacterPrompt,
    updateNegativePrompt,
    updateNote,
    updatePositivePrompt,
    mutableRowState,
  } from "../../api";
  import { app, errorText, setNotice } from "../../stores/app-state.svelte";
  import { beginFileDrag } from "../../stores/file-drag";
  import { requestDelete } from "../../stores/delete-actions.svelte";
  import { removeFromGroup } from "../../stores/group-store.svelte";
  import { patchRowFields, patchRowTags, resetRows, rowStore } from "../../stores/row-store.svelte";
  import { loadTags, tagStore } from "../../stores/tag-store.svelte";
  import { thumbnails } from "../../images/thumbnails";
  import {
    detailPreviews,
    galleryPreviews,
    originalImages,
  } from "../../images/progressive-images";
  import { vibeStatuses } from "../../images/vibe-statuses";
  import { recordRowStateChange } from "../../stores/history-actions";
  import { softFade, softPop } from "../../ui/motion";

  const row = $derived(rowStore.activeRow);
  const hasImage = $derived(
    Boolean(row && (row.imagePath?.trim() || row.storedImagePath?.trim())),
  );

  let thumbUrl = $state<string | null>(null);
  let galleryUrl = $state<string | null>(null);
  let previewUrl = $state<string | null>(null);
  let originalUrl = $state<string | null>(null);
  let previewError = $state<string | null>(null);
  let originalError = $state<string | null>(null);
  let vibeRefs = $state<number | null>(null);
  let lightboxOpen = $state(false);
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let tagQuery = $state("");
  let tagInputFocused = $state(false);
  let copiedField = $state<string | null>(null);
  let copyTimer = 0;

  async function recordOrWarn(label: string, before: ReturnType<typeof mutableRowState>[]): Promise<void> {
    try {
      await recordRowStateChange(label, before);
    } catch (historyError) {
      setNotice({
        tone: "error",
        text: `操作已完成，但未能记录撤销历史：${errorText(historyError)}`,
      });
    }
  }

  function createPromptEditor(
    label: string,
    getField: () => string | null | undefined,
    saveFn: (rowId: number, value: string) => Promise<any>,
    patchField: (rowId: number, value: string, result: any) => void,
  ) {
    let editing = $state(false);
    let value = $state("");
    let saving = $state(false);
    let error = $state<string | null>(null);

    function start(): void {
      if (!row) return;
      value = getField() ?? "";
      editing = true;
      error = null;
    }

    function cancel(): void {
      editing = false;
      error = null;
    }

    async function save(): Promise<void> {
      const current = row;
      if (!current || saving) return;
      saving = true;
      error = null;
      const before = mutableRowState(current);
      try {
        const result = await saveFn(current.id, value);
        editing = false;
        patchField(current.id, value, result);
        await recordOrWarn(label, [before]);
      } catch (e) {
        error = errorText(e);
      } finally {
        saving = false;
      }
    }

    function reset(): void {
      editing = false;
      error = null;
    }

    return {
      get editing() { return editing; },
      get value() { return value; },
      set value(v: string) { value = v; },
      get saving() { return saving; },
      get error() { return error; },
      start,
      cancel,
      save,
      reset,
    };
  }

  const promptEditor = createPromptEditor(
    "编辑正向提示词",
    () => row?.positivePrompt,
    updatePositivePrompt,
    (id, value, result) => patchRowFields(id, { positivePrompt: value, artists: result.newArtists }),
  );

  const negPromptEditor = createPromptEditor(
    "编辑负向提示词",
    () => row?.negativePrompt,
    updateNegativePrompt,
    (id, value) => patchRowFields(id, { negativePrompt: value }),
  );

  const characterPromptEditor = createPromptEditor(
    "编辑角色提示词",
    () => row?.characterPrompt,
    updateCharacterPrompt,
    (id, value, result) =>
      patchRowFields(id, { characterPrompt: value, artists: result.newArtists }),
  );

  const noteEditor = createPromptEditor(
    "编辑备注",
    () => row?.note,
    updateNote,
    (id, value) => patchRowFields(id, { note: value.trim() || null }),
  );

  // 切换行时重置并加载图片：先用缓存缩略图占位，大图就绪后替换
  $effect(() => {
    thumbUrl = null;
    galleryUrl = null;
    previewUrl = null;
    originalUrl = null;
    previewError = null;
    originalError = null;
    vibeRefs = null;
    lightboxOpen = false;
    tagQuery = "";
    saveError = null;
    promptEditor.reset();
    characterPromptEditor.reset();
    negPromptEditor.reset();
    noteEditor.reset();
    const current = row;
    if (!current || !hasImage) {
      detailPreviews.retain(new Set());
      return;
    }
    const rowId = current.id;
    detailPreviews.retain(new Set([rowId]));
    let cancelled = false;
    galleryUrl = galleryPreviews.cached(rowId);
    void thumbnails.load(rowId).then(
      url => {
        if (!cancelled) {
          thumbUrl = url;
        }
      },
      () => {},
    );
    void vibeStatuses.load(rowId).then(
      count => {
        if (!cancelled) {
          vibeRefs = count;
        }
      },
      () => {},
    );
    void detailPreviews.load(rowId, true).then(
      url => {
        if (!cancelled) {
          previewUrl = url;
        }
      },
      error => {
        if (!cancelled) {
          previewError = errorText(error);
        }
      },
    );
    return () => {
      cancelled = true;
      detailPreviews.retain(new Set());
    };
  });

  $effect(() => {
    const current = row;
    if (!lightboxOpen || !current || !hasImage) {
      originalImages.retain(new Set());
      return;
    }
    const rowId = current.id;
    originalImages.retain(new Set([rowId]));
    let cancelled = false;
    originalError = null;
    void originalImages.load(rowId, true).then(
      url => {
        if (!cancelled) {
          originalUrl = url;
        }
      },
      error => {
        if (!cancelled) {
          originalError = errorText(error);
        }
      },
    );
    return () => {
      cancelled = true;
      originalImages.retain(new Set());
    };
  });

  const displayUrl = $derived(previewUrl ?? galleryUrl ?? thumbUrl);
  const lightboxUrl = $derived(originalUrl ?? displayUrl);

  const suggestions = $derived.by(() => {
    if (!row) {
      return [] as string[];
    }
    const query = tagQuery.trim().toLocaleLowerCase();
    const existing = new Set(row.tags);
    return tagStore.list
      .filter(tag => !existing.has(tag.name))
      .filter(tag => query === "" || tag.name.toLocaleLowerCase().includes(query))
      .map(tag => tag.name);
  });

  async function applyTags(next: string[]): Promise<void> {
    const current = row;
    if (!current || saving) {
      return;
    }
    saving = true;
    saveError = null;
    const before = mutableRowState(current);
    try {
      await setTagsForRow(current.id, next);
      patchRowTags(current.id, next);
      await loadTags();
      await recordOrWarn("编辑 Tag", [before]);
    } catch (error) {
      saveError = errorText(error);
    } finally {
      saving = false;
    }
  }

  async function addTag(name: string): Promise<void> {
    const current = row;
    const normalized = name.trim();
    if (!current || !normalized || current.tags.includes(normalized) || saving) {
      return;
    }
    tagQuery = "";
    saving = true;
    saveError = null;
    const before = mutableRowState(current);
    try {
      if (!tagStore.list.some(tag => tag.name === normalized)) {
        await createTag(normalized);
      }
      const next = [...current.tags, normalized];
      await setTagsForRow(current.id, next);
      patchRowTags(current.id, next);
      await loadTags();
      await recordOrWarn("添加 Tag", [before]);
    } catch (error) {
      saveError = errorText(error);
    } finally {
      saving = false;
    }
  }

  function removeTag(name: string): void {
    if (row) {
      void applyTags(row.tags.filter(tag => tag !== name));
    }
  }

  async function ungroupCurrent(): Promise<void> {
    const current = row;
    if (!current) return;
    const before = mutableRowState(current);
    await removeFromGroup({ kind: "explicit", rowIds: [current.id] });
    resetRows();
    await recordOrWarn("取消分组", [before]);
  }

  function onTagInputKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      event.preventDefault();
      void addTag(tagQuery);
    } else if (event.key === "Escape") {
      tagQuery = "";
      (event.target as HTMLInputElement).blur();
    }
  }

  async function copyField(label: string, value: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(value);
      copiedField = label;
      window.clearTimeout(copyTimer);
      copyTimer = window.setTimeout(() => {
        copiedField = null;
      }, 1200);
    } catch {
      copiedField = null;
    }
  }
</script>

<svelte:window
  onkeydown={event => {
    if (event.key === "Escape" && lightboxOpen) {
      lightboxOpen = false;
    }
  }}
/>

<div class="detail-panel">
  <header class="panel-header">
    <h3>{row ? `第 ${row.sourceOrdinal} 行` : "详情"}</h3>
    <div class="panel-actions">
      {#if row}
        <button
          type="button"
          class="btn btn-danger delete-btn"
          onclick={() => requestDelete({ kind: "explicit", rowIds: [row.id] }, 1)}
        >
          删除
        </button>
      {/if}
      <button
        type="button"
        class="btn btn-ghost collapse-btn"
        title="收起详情面板"
        onclick={() => (app.detailOpen = false)}
      >
        »
      </button>
    </div>
  </header>

  {#if !row}
    <div class="panel-empty">
      <p class="faint">点击图片或行查看详情</p>
    </div>
  {:else}
    <div class="panel-scroll">
      <div class="preview-box">
        {#if displayUrl}
          <button
            type="button"
            class="preview-btn"
            title="点击放大"
            onclick={() => (lightboxOpen = true)}
          >
            <img
              src={displayUrl}
              alt="第 {row.sourceOrdinal} 行图片"
              draggable="false"
              onmousedown={(e) => { if (row && hasImage) beginFileDrag(e, row.id); }}
            />
            {#if vibeRefs}
              <span
                class="vibe-badge"
                title="原图元数据包含 {vibeRefs} 个 vibe 引用，拖到 NovelAI 可一并导入"
                transition:softPop={{ duration: 140, y: 0, start: 0.85 }}
              >VIBE ×{vibeRefs}</span>
            {/if}
          </button>
        {:else if !hasImage}
          <span class="faint">无图片</span>
        {:else if previewError}
          <span class="faint" title={previewError}>图片不可用</span>
        {:else}
          <span class="faint">正在加载图片…</span>
        {/if}
      </div>

      <section class="field note-field">
        <div class="field-head">
          <h4>备注</h4>
          <div class="field-head-actions">
            {#if !noteEditor.editing}
              <button type="button" class="copy-btn" onclick={noteEditor.start}>编辑</button>
            {/if}
            {#if row.note && !noteEditor.editing}
              <button type="button" class="copy-btn" onclick={() => void copyField("备注", row.note ?? "")}>
                {copiedField === "备注" ? "已复制" : "复制"}
              </button>
            {/if}
          </div>
        </div>
        {#if noteEditor.editing}
          <textarea
            class="prompt-textarea note-textarea"
            placeholder="输入备注；导出智绘姬 JSON 时会作为预设名称"
            bind:value={noteEditor.value}
            disabled={noteEditor.saving}
            onkeydown={event => {
              if (event.key === "Escape") noteEditor.cancel();
            }}
            transition:softFade={{ duration: 130 }}
          ></textarea>
          <div class="prompt-edit-actions">
            <button type="button" class="btn btn-sm" disabled={noteEditor.saving} onclick={noteEditor.cancel}>取消</button>
            <button type="button" class="btn btn-sm btn-primary" disabled={noteEditor.saving} onclick={() => void noteEditor.save()}>
              {noteEditor.saving ? "保存中…" : "保存"}
            </button>
          </div>
          {#if noteEditor.error}
            <p class="save-error">{noteEditor.error}</p>
          {/if}
        {:else}
          <pre class:is-empty={!row.note} transition:softFade={{ duration: 130 }}>{row.note ?? "—"}</pre>
        {/if}
      </section>

      <section class="tag-editor">
        <h4>Tags</h4>
        <div class="chip-list">
          {#each row.tags as tag (tag)}
            <span class="chip">
              {tag}
              <button
                type="button"
                aria-label="移除 Tag {tag}"
                disabled={saving}
                onclick={() => removeTag(tag)}
              >
                ×
              </button>
            </span>
          {:else}
            <span class="faint">尚无 Tag</span>
          {/each}
        </div>
        <div class="tag-input-wrap">
          <input
            type="text"
            placeholder="输入 Tag，回车即建即贴…"
            bind:value={tagQuery}
            disabled={saving}
            onkeydown={onTagInputKeydown}
            onfocus={() => (tagInputFocused = true)}
            onblur={() => window.setTimeout(() => (tagInputFocused = false), 120)}
          />
          {#if tagInputFocused && suggestions.length > 0}
            <div class="suggestions" transition:softPop={{ duration: 145, y: -4, start: 0.98 }}>
              {#each suggestions as name (name)}
                <button type="button" onmousedown={event => event.preventDefault()} onclick={() => void addTag(name)}>
                  {name}
                </button>
              {/each}
            </div>
          {/if}
        </div>
        {#if saveError}
          <p class="save-error">保存失败：{saveError}</p>
        {/if}
      </section>

      <section class="field">
        <div class="field-head">
          <h4>分组</h4>
        </div>
        <div class="group-info">
          {#if row.groupName}
            <span class="group-badge">{row.groupName}</span>
            <button type="button" class="copy-btn" onclick={() => void ungroupCurrent()}>取消分组</button>
          {:else}
            <span class="faint-text">未分组</span>
          {/if}
        </div>
      </section>

      <section class="field">
        <div class="field-head">
          <h4>时间</h4>
          {#if row.time}
            <button type="button" class="copy-btn" onclick={() => void copyField("时间", row.time ?? "")}>
              {copiedField === "时间" ? "已复制" : "复制"}
            </button>
          {/if}
        </div>
        <pre class:is-empty={!row.time}>{row.time ?? "—"}</pre>
      </section>

      {#each [
        { label: "正向提示词", text: row.positivePrompt, editor: promptEditor },
        { label: "角色提示词", text: row.characterPrompt, editor: characterPromptEditor },
        { label: "负向提示词", text: row.negativePrompt, editor: negPromptEditor },
      ] as prompt (prompt.label)}
        <section class="field">
          <div class="field-head">
            <h4>{prompt.label}</h4>
            <div class="field-head-actions">
              {#if !prompt.editor.editing}
                <button type="button" class="copy-btn" onclick={prompt.editor.start}>编辑</button>
              {/if}
              {#if prompt.text && !prompt.editor.editing}
                <button type="button" class="copy-btn" onclick={() => void copyField(prompt.label, prompt.text ?? "")}>
                  {copiedField === prompt.label ? "已复制" : "复制"}
                </button>
              {/if}
            </div>
          </div>
          {#if prompt.editor.editing}
            <textarea
              class="prompt-textarea"
              bind:value={prompt.editor.value}
              disabled={prompt.editor.saving}
              onkeydown={event => {
                if (event.key === "Escape") prompt.editor.cancel();
              }}
              transition:softFade={{ duration: 130 }}
            ></textarea>
            <div class="prompt-edit-actions">
              <button type="button" class="btn btn-sm" disabled={prompt.editor.saving} onclick={prompt.editor.cancel}>取消</button>
              <button type="button" class="btn btn-sm btn-primary" disabled={prompt.editor.saving} onclick={() => void prompt.editor.save()}>
                {prompt.editor.saving ? "保存中…" : "保存"}
              </button>
            </div>
            {#if prompt.editor.error}
              <p class="save-error">{prompt.editor.error}</p>
            {/if}
          {:else}
            <pre class:is-empty={!prompt.text} transition:softFade={{ duration: 130 }}>{prompt.text ?? "—"}</pre>
          {/if}
        </section>
      {/each}

      {#each [{ label: "画师串", value: row.artists }, { label: "图片文件夹", value: row.imageFolder }, { label: "图片路径", value: row.imagePath }] as field (field.label)}
        <section class="field">
          <div class="field-head">
            <h4>{field.label}</h4>
            {#if field.value}
              <button type="button" class="copy-btn" onclick={() => void copyField(field.label, field.value ?? "")}>
                {copiedField === field.label ? "已复制" : "复制"}
              </button>
            {/if}
          </div>
          <pre class:is-empty={!field.value}>{field.value ?? "—"}</pre>
        </section>
      {/each}
    </div>
  {/if}
</div>

{#if lightboxOpen && lightboxUrl && row}
  <div
    class="lightbox"
    role="dialog"
    aria-modal="true"
    aria-label="图片放大预览"
    tabindex="-1"
    onclick={() => (lightboxOpen = false)}
    onkeydown={event => {
      if (event.key === "Escape" || event.key === "Enter") {
        lightboxOpen = false;
      }
    }}
    transition:softFade={{ duration: 150 }}
  >
    <img
      src={lightboxUrl}
      alt="第 {row.sourceOrdinal} 行大图"
      title={originalError ?? (originalUrl ? "完整原图" : "正在加载完整原图…")}
      draggable="false"
      onmousedown={(e) => { if (row && hasImage) beginFileDrag(e, row.id); }}
      transition:softPop={{ duration: 190, y: 0, start: 0.98 }}
    />
  </div>
{/if}

<style>
  .detail-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 8px 8px 14px;
    flex: none;
  }

  .panel-header h3 {
    font-size: var(--font-md);
    font-weight: 600;
  }

  .collapse-btn {
    padding: 2px 8px;
    font-size: var(--font-base);
  }

  .panel-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .delete-btn {
    padding: 2px 8px;
    font-size: var(--font-sm);
  }

  .panel-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .panel-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .preview-box {
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--surface-2);
    border-radius: var(--radius-m);
    height: clamp(180px, 36vh, 320px);
    flex: none;
    overflow: hidden;
  }

  .preview-btn {
    border: none;
    padding: 0;
    background: none;
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
    overflow: hidden;
  }

  .vibe-badge {
    position: absolute;
    top: 6px;
    right: 6px;
    padding: 2px 8px;
    border-radius: var(--radius-m);
    background: rgba(90, 60, 160, 0.85);
    color: #fff;
    font-size: var(--font-sm);
    font-weight: 600;
    letter-spacing: 0.5px;
    pointer-events: none;
  }

  .preview-btn img {
    width: auto;
    height: auto;
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    display: block;
  }

  .tag-editor h4,
  .field h4 {
    font-size: var(--font-sm);
    color: var(--text-2);
    font-weight: 600;
    margin-bottom: 6px;
  }

  .chip-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 8px;
    font-size: var(--font-sm);
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px 2px 10px;
    max-width: none;
  }

  .chip button {
    border: none;
    background: none;
    color: inherit;
    font-size: var(--font-md);
    line-height: 1;
    padding: 0 2px;
    border-radius: 50%;
  }

  .chip button:hover:not(:disabled) {
    background: var(--accent-soft-border);
  }

  .tag-input-wrap {
    position: relative;
  }

  .tag-input-wrap input {
    width: 100%;
    padding: 6px 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    background: var(--surface);
    font-size: var(--font-md);
  }

  .tag-input-wrap input:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: var(--focus-ring);
  }

  .suggestions {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    z-index: var(--z-dropdown);
    background: var(--surface);
    border-radius: var(--radius-s);
    box-shadow: var(--shadow-2);
    max-height: 240px;
    overflow-y: auto;
    overscroll-behavior: contain;
    display: flex;
    flex-direction: column;
  }

  .suggestions button {
    flex: none;
    border: none;
    background: none;
    text-align: left;
    padding: 6px 10px;
    font-size: var(--font-md);
    transition: background var(--motion-fast) var(--ease-responsive);
  }

  .suggestions button:hover {
    background: var(--accent-soft);
  }

  .save-error {
    margin-top: 6px;
    font-size: var(--font-sm);
    color: var(--danger);
  }

  .group-info {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--font-sm);
  }

  .group-badge {
    background: var(--accent-soft);
    color: var(--accent);
    padding: 2px 8px;
    border-radius: var(--radius-full);
    font-size: var(--font-sm);
  }

  .faint-text {
    color: var(--text-3);
    font-size: var(--font-sm);
  }

  .field-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .field-head-actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .prompt-textarea {
    width: 100%;
    min-height: 120px;
    padding: 8px 10px;
    font-family: inherit;
    font-size: var(--font-sm);
    border: 1px solid var(--accent);
    border-radius: var(--radius-s);
    background: var(--surface);
    resize: vertical;
  }

  .prompt-textarea:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent-soft);
  }

  .note-textarea {
    min-height: 72px;
  }

  .prompt-edit-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    margin-top: 6px;
  }

  .btn-sm {
    padding: 3px 10px;
    font-size: var(--font-sm);
  }

  .btn-primary {
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }

  .btn-primary:hover:not(:disabled) {
    filter: brightness(1.1);
  }

  .copy-btn {
    border: none;
    background: none;
    color: var(--accent);
    font-size: var(--font-sm);
    padding: 0;
    min-width: 36px;
    transition:
      color var(--motion-fast) var(--ease-responsive),
      opacity var(--motion-fast) var(--ease-responsive);
  }

  .copy-btn:hover {
    text-decoration: underline;
  }

  .field pre {
    margin: 0;
    font-family: inherit;
    font-size: var(--font-sm);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    background: var(--surface-2);
    border-radius: var(--radius-s);
    padding: 8px 10px;
    max-height: 180px;
    overflow-y: auto;
  }

  .field pre.is-empty {
    color: var(--text-3);
  }

  .lightbox {
    position: fixed;
    inset: 0;
    z-index: var(--z-lightbox);
    background: rgb(15 20 28 / 78%);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 32px;
    cursor: zoom-out;
  }

  .lightbox img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: var(--radius-s);
    box-shadow: var(--shadow-2);
  }
</style>
