<script lang="ts">
  import { getRowPreview, setTagsForRow } from "../../api";
  import { binaryBuffer } from "../../image-loader";
  import { app, errorText } from "../app-state.svelte";
  import { patchRowTags, rowStore } from "../row-store.svelte";
  import { loadTags, tagStore } from "../tag-store.svelte";
  import { thumbnails } from "../thumbnails";

  const row = $derived(rowStore.activeRow);
  const hasImage = $derived(
    Boolean(row && (row.imagePath?.trim() || row.storedImagePath?.trim())),
  );

  let thumbUrl = $state<string | null>(null);
  let previewUrl = $state<string | null>(null);
  let previewError = $state<string | null>(null);
  let lightboxOpen = $state(false);
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let tagQuery = $state("");
  let tagInputFocused = $state(false);
  let copiedField = $state<string | null>(null);
  let copyTimer = 0;

  // 切换行时重置并加载图片：先用缓存缩略图占位，大图就绪后替换
  $effect(() => {
    thumbUrl = null;
    previewUrl = null;
    previewError = null;
    lightboxOpen = false;
    tagQuery = "";
    saveError = null;
    const current = row;
    if (!current || !hasImage) {
      return;
    }
    const rowId = current.id;
    let cancelled = false;
    let createdUrl: string | null = null;
    void thumbnails.load(rowId).then(
      url => {
        if (!cancelled) {
          thumbUrl = url;
        }
      },
      () => {},
    );
    void getRowPreview(rowId).then(
      bytes => {
        const url = URL.createObjectURL(new Blob([binaryBuffer(bytes)], { type: "image/png" }));
        if (cancelled) {
          URL.revokeObjectURL(url);
          return;
        }
        createdUrl = url;
        previewUrl = url;
      },
      error => {
        if (!cancelled) {
          previewError = errorText(error);
        }
      },
    );
    return () => {
      cancelled = true;
      if (createdUrl) {
        URL.revokeObjectURL(createdUrl);
      }
    };
  });

  const displayUrl = $derived(previewUrl ?? thumbUrl);

  const fields = $derived(
    row
      ? [
          { label: "时间", value: row.time },
          { label: "正向提示词", value: row.positivePrompt },
          { label: "负向提示词", value: row.negativePrompt },
          { label: "画师串", value: row.artists },
          { label: "图片文件夹", value: row.imageFolder },
          { label: "图片路径", value: row.imagePath },
        ]
      : [],
  );

  const suggestions = $derived.by(() => {
    if (!row) {
      return [] as string[];
    }
    const query = tagQuery.trim().toLocaleLowerCase();
    const existing = new Set(row.tags);
    return tagStore.list
      .filter(tag => !existing.has(tag.name))
      .filter(tag => query === "" || tag.name.toLocaleLowerCase().includes(query))
      .slice(0, 8)
      .map(tag => tag.name);
  });

  async function applyTags(next: string[]): Promise<void> {
    const current = row;
    if (!current || saving) {
      return;
    }
    saving = true;
    saveError = null;
    try {
      await setTagsForRow(current.id, next);
      patchRowTags(current.id, next);
      await loadTags();
    } catch (error) {
      saveError = errorText(error);
    } finally {
      saving = false;
    }
  }

  function addTag(name: string): void {
    tagQuery = "";
    if (row && !row.tags.includes(name)) {
      void applyTags([...row.tags, name]);
    }
  }

  function removeTag(name: string): void {
    if (row) {
      void applyTags(row.tags.filter(tag => tag !== name));
    }
  }

  function onTagInputKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      event.preventDefault();
      const exact = suggestions.find(name => name === tagQuery.trim());
      const first = exact ?? suggestions[0];
      if (first) {
        addTag(first);
      }
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
    <button
      type="button"
      class="btn btn-ghost collapse-btn"
      title="收起详情面板"
      onclick={() => (app.detailOpen = false)}
    >
      »
    </button>
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
            <img src={displayUrl} alt="第 {row.sourceOrdinal} 行图片" />
          </button>
        {:else if !hasImage}
          <span class="faint">无图片</span>
        {:else if previewError}
          <span class="faint" title={previewError}>图片不可用</span>
        {:else}
          <span class="faint">正在加载图片…</span>
        {/if}
      </div>

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
            placeholder={tagStore.list.length === 0 ? "请先在左侧 Tag 库新建 Tag" : "添加已有 Tag…"}
            bind:value={tagQuery}
            disabled={saving || tagStore.list.length === 0}
            onkeydown={onTagInputKeydown}
            onfocus={() => (tagInputFocused = true)}
            onblur={() => window.setTimeout(() => (tagInputFocused = false), 120)}
          />
          {#if tagInputFocused && suggestions.length > 0}
            <div class="suggestions">
              {#each suggestions as name (name)}
                <button type="button" onmousedown={event => event.preventDefault()} onclick={() => addTag(name)}>
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

      {#each fields as field (field.label)}
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

{#if lightboxOpen && displayUrl && row}
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
  >
    <img src={displayUrl} alt="第 {row.sourceOrdinal} 行大图" />
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
    border-bottom: 1px solid var(--border);
    flex: none;
  }

  .panel-header h3 {
    font-size: 13px;
    font-weight: 600;
  }

  .collapse-btn {
    padding: 2px 8px;
    font-size: 14px;
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
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    min-height: 180px;
    overflow: hidden;
  }

  .preview-btn {
    border: none;
    padding: 0;
    background: none;
    width: 100%;
    display: flex;
    justify-content: center;
  }

  .preview-btn img {
    width: 100%;
    max-height: 320px;
    object-fit: contain;
    display: block;
  }

  .tag-editor h4,
  .field h4 {
    font-size: 12px;
    color: var(--text-2);
    font-weight: 600;
    margin-bottom: 6px;
  }

  .chip-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 8px;
    font-size: 12px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: var(--accent-soft);
    color: var(--accent);
    border-radius: 999px;
    padding: 2px 6px 2px 10px;
  }

  .chip button {
    border: none;
    background: none;
    color: inherit;
    font-size: 13px;
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
    font-size: 13px;
  }

  .tag-input-wrap input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .suggestions {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    z-index: 20;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    box-shadow: var(--shadow-2);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .suggestions button {
    border: none;
    background: none;
    text-align: left;
    padding: 6px 10px;
    font-size: 13px;
  }

  .suggestions button:hover {
    background: var(--accent-soft);
  }

  .save-error {
    margin-top: 6px;
    font-size: 12px;
    color: var(--danger);
  }

  .field-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .copy-btn {
    border: none;
    background: none;
    color: var(--accent);
    font-size: 12px;
    padding: 0;
  }

  .copy-btn:hover {
    text-decoration: underline;
  }

  .field pre {
    margin: 0;
    font-family: inherit;
    font-size: 12.5px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    background: var(--surface-2);
    border: 1px solid var(--border);
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
    z-index: 90;
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
