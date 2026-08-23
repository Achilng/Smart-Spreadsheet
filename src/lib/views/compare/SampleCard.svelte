<script lang="ts">
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import type { RowRecord } from "../../api";
  import { detailPreviews } from "../../images/progressive-images";
  import { thumbnails } from "../../images/thumbnails";
  import { formatCount, setNotice } from "../../stores/app-state.svelte";
  import { modelVersionBadge } from "../../utils/model-version";
  import { rowFileName, rowResolution } from "../../utils/row-display";

  let {
    row,
    refreshing,
    onrefresh,
  }: {
    row: RowRecord;
    refreshing: boolean;
    onrefresh: () => void;
  } = $props();

  const hasImage = $derived(
    Boolean(row.imagePath?.trim() || row.storedImagePath?.trim()),
  );
  const versionBadge = $derived(modelVersionBadge(row.generationModel));
  const title = $derived(rowFileName(row) ?? `第 ${row.sourceOrdinal} 张`);

  let displayUrl = $state<string | null>(null);
  let loadFailed = $state(false);

  // 渐进加载：256px 缩略图占位，1024px 高清层到达后交叉替换。
  $effect(() => {
    displayUrl = null;
    loadFailed = false;
    const rowId = row.id;
    if (!hasImage) {
      return;
    }
    const cachedThumb = thumbnails.cached(rowId);
    if (cachedThumb) {
      displayUrl = cachedThumb;
    } else {
      let cancelled = false;
      void thumbnails.load(rowId).then(
        loaded => {
          if (!cancelled) {
            displayUrl = loaded;
          }
        },
        () => {},
      );
      return () => {
        cancelled = true;
      };
    }
    let cancelledLate = false;
    void detailPreviews.load(rowId, true).then(
      loaded => {
        if (!cancelledLate) {
          displayUrl = loaded;
        }
      },
      () => {},
    );
    return () => {
      cancelledLate = true;
    };
  });

  let promptsOpen = $state(false);

  const promptFields = $derived([
    { label: "正向提示词", value: row.positivePrompt },
    { label: "角色提示词", value: row.characterPrompt },
    { label: "负向提示词", value: row.negativePrompt },
  ]);

  async function copyText(text: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
      setNotice({ tone: "success", text: "已复制到剪贴板。" });
    } catch {
      setNotice({ tone: "error", text: "复制失败，请检查剪贴板权限。" });
    }
  }
</script>

<section class="sample-card">
  <div class="sample-media">
    {#if displayUrl}
      <img src={displayUrl} alt={title} draggable="false" />
    {:else if loadFailed || !hasImage}
      <span class="media-note">{hasImage ? "图片不可用" : "无图"}</span>
    {:else}
      <span class="media-loading" aria-hidden="true"></span>
    {/if}
    {#if versionBadge}
      <span
        class="version-badge {versionBadge.className}"
        title={"作画模型：" + (row.generationModel ?? "")}
      >{versionBadge.label}</span>
    {/if}
    {#if row.vibeReferenceCount}
      <span
        class="vibe-badge"
        title="原图元数据包含 {row.vibeReferenceCount} 个 vibe 引用"
      >VIBE ×{row.vibeReferenceCount}</span>
    {/if}
  </div>

  <div class="sample-info">
    <header class="sample-head">
      <h1 class="sample-title" title={row.imagePath ?? undefined}>{title}</h1>
      <button
        type="button"
        class="refresh"
        onclick={onrefresh}
        disabled={refreshing}
        title="重新拉取样本与全部分区"
      >
        <RefreshCw size={14} strokeWidth={2} />
        {refreshing ? "刷新中…" : "刷新"}
      </button>
    </header>
    <div class="sample-facts">
      {#if rowResolution(row)}<span class="tabular">{rowResolution(row)}</span>{/if}
      {#if row.time}<span>{row.time}</span>{/if}
      {#if row.vibeReferenceCount != null}
        <span>VIBE 引用 {formatCount(row.vibeReferenceCount)} 个</span>
      {/if}
      {#if row.generationModel}<span title={row.generationModel}>{row.generationModel}</span>{/if}
    </div>
    {#if row.artists?.trim()}
      <div class="sample-artists">
        <span class="fact-label">画师串</span>
        <span class="artists-value">{row.artists.trim()}</span>
        <button
          type="button"
          class="mini-copy"
          title="复制画师串"
          onclick={() => void copyText(row.artists!.trim())}
        >复制</button>
      </div>
    {/if}

    <button type="button" class="prompts-toggle" onclick={() => (promptsOpen = !promptsOpen)}>
      {promptsOpen ? "收起提示词" : "展开提示词"}
    </button>
    {#if promptsOpen}
      <div class="prompt-list">
        {#each promptFields as field (field.label)}
          <div class="prompt-field">
            <div class="prompt-head">
              <span class="fact-label">{field.label}</span>
              {#if field.value}
                <button
                  type="button"
                  class="mini-copy"
                  onclick={() => void copyText(field.value!)}
                >复制</button>
              {/if}
            </div>
            <pre class="prompt-value">{field.value ?? "（空）"}</pre>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</section>

<style>
  .sample-card {
    display: flex;
    gap: 20px;
    padding: 24px 26px;
    align-items: flex-start;
  }

  .sample-media {
    position: relative;
    flex: none;
    width: 220px;
    height: 220px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--surface);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-1);
    overflow: hidden;
  }

  .sample-media img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    display: block;
  }

  .media-loading {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    border: 3px solid var(--surface-2);
    border-top-color: var(--text-4);
    animation: sample-spin 0.9s linear infinite;
  }

  @keyframes sample-spin {
    to {
      transform: rotate(360deg);
    }
  }

  .media-note {
    font-size: var(--font-sm);
    color: var(--text-3);
  }

  .vibe-badge {
    position: absolute;
    top: 8px;
    right: 8px;
    z-index: 1;
  }

  .version-badge {
    position: absolute;
    top: 8px;
    left: 8px;
    z-index: 1;
  }

  .sample-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .sample-head {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .sample-title {
    margin: 0;
    font-size: 1.2rem;
    font-weight: 700;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .refresh {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text-2);
    font-size: var(--font-xs);
    font-weight: 600;
    padding: 5px 10px;
    border-radius: 999px;
    cursor: pointer;
  }

  .refresh:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--text);
  }

  .refresh:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .sample-facts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
    font-size: var(--font-xs);
    color: var(--text-3);
  }

  .sample-facts span {
    max-width: 100%;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sample-artists {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
    background: var(--surface);
    border-radius: var(--radius-s);
    padding: 8px 10px;
  }

  .fact-label {
    flex: none;
    font-size: var(--font-xs);
    font-weight: 700;
    color: var(--text-3);
  }

  .artists-value {
    flex: 1;
    min-width: 0;
    font-size: var(--font-xs);
    color: var(--text);
    white-space: pre-line;
    word-break: break-all;
    max-height: 6.4em;
    overflow: auto;
  }

  .mini-copy {
    flex: none;
    border: none;
    background: transparent;
    color: var(--accent);
    font-size: var(--font-xs);
    font-weight: 600;
    padding: 2px 6px;
    cursor: pointer;
    border-radius: var(--radius-s);
  }

  .mini-copy:hover {
    background: var(--surface-2);
  }

  .prompts-toggle {
    align-self: flex-start;
    border: none;
    background: transparent;
    color: var(--accent);
    font-size: var(--font-sm);
    font-weight: 600;
    padding: 4px 8px;
    cursor: pointer;
    border-radius: var(--radius-s);
  }

  .prompts-toggle:hover {
    background: var(--surface-2);
  }

  .prompt-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .prompt-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .prompt-value {
    margin: 4px 0 0;
    font-family: inherit;
    font-size: var(--font-xs);
    line-height: 1.6;
    color: var(--text-2);
    white-space: pre-wrap;
    word-break: break-all;
    background: var(--surface);
    border-radius: var(--radius-s);
    padding: 10px 12px;
    max-height: 180px;
    overflow: auto;
  }

  @media (max-width: 720px) {
    .sample-card {
      flex-direction: column;
    }

    .sample-media {
      width: 100%;
      height: 240px;
    }
  }
</style>
