<script lang="ts">
  import DetailPanelLayout from "../../ui/DetailPanelLayout.svelte";
  import DetailPreview from "../../ui/DetailPreview.svelte";
  import DetailLightbox from "../../ui/DetailLightbox.svelte";
  import { rowFileName, rowResolution } from "../../utils/row-display";
  import { modelVersionBadge } from "../../utils/model-version";
  import X from "@lucide/svelte/icons/x";
  import { untrack } from "svelte";

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
  import { removeFromGroup, groupStore } from "../../stores/group-store.svelte";
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
  const versionBadge = $derived(
    row ? modelVersionBadge(row.generationModel) : null,
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
    let restored = $state(false);
    let initialValue = "";
    let editingRowId: number | null = null;
    /** 切行时未保存的编辑按行暂存，回到该行再点“编辑”可继续 */
    const drafts = new Map<number, string>();

    function start(): void {
      if (!row) return;
      const base = getField() ?? "";
      const draft = drafts.get(row.id);
      value = draft ?? base;
      initialValue = base;
      restored = draft !== undefined && draft !== base;
      editingRowId = row.id;
      editing = true;
      error = null;
    }

    function isDirty(): boolean {
      return editing && value !== initialValue;
    }

    function cancel(): void {
      if (isDirty() && !window.confirm(`放弃「${label.replace("编辑", "")}」未保存的修改吗？`)) {
        return;
      }
      if (editingRowId !== null) drafts.delete(editingRowId);
      editing = false;
      error = null;
      restored = false;
    }

    async function save(): Promise<void> {
      const current = row;
      if (!current || saving) return;
      saving = true;
      error = null;
      const before = mutableRowState(current);
      try {
        const result = await saveFn(current.id, value);
        drafts.delete(current.id);
        // 先把新值写入行缓存，再退出编辑态，避免展示态短暂回显旧值。
        patchField(current.id, value, result);
        editing = false;
        restored = false;
        await recordOrWarn(label, [before]);
      } catch (e) {
        error = errorText(e);
      } finally {
        saving = false;
      }
    }

    function onKeydown(event: KeyboardEvent): void {
      // 中文输入法用 Enter 确认候选词时不应触发保存。
      if (event.isComposing) return;
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        cancel();
      } else if (event.key === "Enter" && !event.shiftKey) {
        event.preventDefault();
        event.stopPropagation();
        void save();
      }
    }

    /** 切换行时调用：不丢内容，未保存的编辑暂存为原行草稿。 */
    function reset(): void {
      if (isDirty() && editingRowId !== null) {
        drafts.set(editingRowId, value);
      }
      editing = false;
      error = null;
      restored = false;
    }

    return {
      get editing() { return editing; },
      get value() { return value; },
      set value(v: string) { value = v; },
      get saving() { return saving; },
      get error() { return error; },
      get restored() { return restored; },
      start,
      cancel,
      save,
      onKeydown,
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

  // 切换行时重置并加载图片：先用缓存缩略图占位，大图就绪后替换。
  // 编辑器 reset 内部会读取 editing/value（脏状态判断），必须 untrack，
  // 否则 effect 依赖 editing——点“编辑”会触发本 effect 重跑并立刻把编辑器关掉。
  $effect(() => {
    const current = row;
    void hasImage;
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
    untrack(() => {
      promptEditor.reset();
      characterPromptEditor.reset();
      negPromptEditor.reset();
      noteEditor.reset();
    });
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
    groupStore.error = null;
    const affected = await removeFromGroup({ kind: "explicit", rowIds: [current.id] });
    if (affected === 0 && groupStore.error) {
      // removeFromGroup 内部吞掉异常并写入 groupStore.error：失败时不能记撤销历史
      setNotice({ tone: "error", text: `取消分组失败：${groupStore.error}` });
      return;
    }
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
      setNotice({ tone: "error", text: "复制失败，请检查剪贴板权限。" });
    }
  }

</script>

<DetailPanelLayout title={row ? (rowFileName(row) ?? `第 ${row.sourceOrdinal} 行`) : "详情"}
  subtitle={row ? [rowResolution(row), row.time].filter(Boolean).join(" · ") : undefined}
  empty={!row} oncollapse={() => app.detailOpen = false}
  ondelete={() => { if (row) requestDelete({ kind: "explicit", rowIds: [row.id] }, 1); }}>
  {#if row}
      <DetailPreview src={displayUrl} alt={`第 ${row.sourceOrdinal} 行图片`}
        placeholder={hasImage ? "正在加载图片…" : "无图片"} error={previewError}
        onopen={() => lightboxOpen = true}
        onmousedown={event => { if (row && hasImage) beginFileDrag(event, row.id); }}>
        {#snippet badges()}
          {#if versionBadge}<span class="version-badge {versionBadge.className}" title={"作画模型：" + (row?.generationModel ?? "")}
            transition:softPop={{ duration: 140, y: 0, start: 0.85 }}>{versionBadge.label}</span>{/if}
          {#if vibeRefs}<span class="vibe-badge" title={`原图元数据包含 ${vibeRefs} 个 vibe 引用，拖到 NovelAI 可一并导入`}
            transition:softPop={{ duration: 140, y: 0, start: 0.85 }}>VIBE ×{vibeRefs}</span>{/if}
        {/snippet}
      </DetailPreview>

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
            title="回车保存，Shift+回车换行"
            onkeydown={noteEditor.onKeydown}
            in:softFade={{ duration: 130 }}
          ></textarea>
          {#if noteEditor.restored}
            <p class="draft-hint">已恢复此前未保存的草稿；「取消」可放弃。</p>
          {/if}
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
          <pre class:is-empty={!row.note}>{row.note ?? "—"}</pre>
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
                <X size={11} strokeWidth={2.2} />
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
              title="回车保存，Shift+回车换行"
              onkeydown={prompt.editor.onKeydown}
              in:softFade={{ duration: 130 }}
            ></textarea>
            {#if prompt.editor.restored}
              <p class="draft-hint">已恢复此前未保存的草稿；「取消」可放弃。</p>
            {/if}
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
            <pre class:is-empty={!prompt.text}>{prompt.text ?? "—"}</pre>
          {/if}
        </section>
      {/each}

      {#if row.generationModel || row.generationSampler || row.generationSteps != null || row.generationSeed}
        <section class="field">
          <div class="field-head">
            <h4>生成信息</h4>
          </div>
          <div class="kv tabular">
            {#if row.generationModel}
              <span class="k">模型</span><span class="v">{row.generationModel}</span>
            {/if}
            {#if row.generationSampler}
              <span class="k">采样器</span><span class="v">{row.generationSampler}</span>
            {/if}
            {#if row.generationSteps != null || row.generationScale}
              <span class="k">步数 / CFG</span>
              <span class="v">{row.generationSteps ?? "—"} / {row.generationScale ?? "—"}</span>
            {/if}
            {#if row.generationSeed}
              <span class="k">种子</span><span class="v">{row.generationSeed}</span>
            {/if}
          </div>
        </section>
      {/if}

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
  {/if}
</DetailPanelLayout>

<DetailLightbox bind:open={lightboxOpen} src={row ? lightboxUrl : null} alt={`第 ${row?.sourceOrdinal} 行大图`}
  title={originalError ?? (originalUrl ? "完整原图" : "正在加载完整原图…")}
  onmousedown={event => { if (row && hasImage) beginFileDrag(event, row.id); }} />
