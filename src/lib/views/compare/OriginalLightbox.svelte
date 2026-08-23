<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import type { RowRecord } from "../../api";
  import {
    detailPreviews,
    originalImages,
  } from "../../images/progressive-images";
  import { thumbnails } from "../../images/thumbnails";
  import { rowFileName } from "../../utils/row-display";

  let {
    row,
    onclose,
  }: {
    row: RowRecord;
    onclose: () => void;
  } = $props();

  let previewUrl = $state<string | null>(null);
  let originalUrl = $state<string | null>(null);
  let originalError = $state<string | null>(null);

  const title = $derived(rowFileName(row) ?? `第 ${row.sourceOrdinal} 张`);
  const displayUrl = $derived(originalUrl ?? previewUrl);

  $effect(() => {
    const rowId = row.id;
    previewUrl = detailPreviews.cached(rowId) ?? thumbnails.cached(rowId);
    originalUrl = originalImages.cached(rowId);
    originalError = null;
    originalImages.retain(new Set([rowId]));
    let cancelled = false;
    void originalImages.load(rowId, true).then(
      url => {
        if (!cancelled) {
          originalUrl = url;
        }
      },
      error => {
        if (!cancelled) {
          originalError = error instanceof Error ? error.message : String(error);
        }
      },
    );
    return () => {
      cancelled = true;
      originalImages.retain(new Set());
    };
  });
</script>

<svelte:window
  onkeydown={event => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onclose();
    }
  }}
/>

<div
  class="lightbox"
  role="dialog"
  aria-modal="true"
  aria-label="{title} 原图"
  tabindex="-1"
  onclick={event => {
    if (event.target === event.currentTarget) {
      onclose();
    }
  }}
  onkeydown={event => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onclose();
    }
  }}
>
  <header class="lightbox-head">
    <div class="lightbox-title" title={title}>{title}</div>
    <div class="lightbox-status" class:error={Boolean(originalError)}>
      {#if originalError}
        原图加载失败，当前显示预览图
      {:else if originalUrl}
        完整原图
      {:else}
        正在加载完整原图…
      {/if}
    </div>
    <button type="button" class="lightbox-close" onclick={onclose} aria-label="关闭原图">
      <X size={20} strokeWidth={1.8} />
    </button>
  </header>

  <div class="lightbox-stage">
    {#if displayUrl}
      <img src={displayUrl} alt="{title} 原图" draggable="false" />
    {:else if originalError}
      <div class="lightbox-empty">无法加载这张图片。</div>
    {:else}
      <span class="lightbox-loading" aria-label="正在加载原图"></span>
    {/if}
  </div>
</div>

<style>
  .lightbox {
    position: fixed;
    inset: 0;
    z-index: var(--z-lightbox);
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 18px 22px 22px;
    background: var(--overlay-heavy);
    color: white;
  }

  .lightbox-head {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .lightbox-title {
    min-width: 0;
    max-width: min(55vw, 720px);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--font-md);
    font-weight: 700;
  }

  .lightbox-status {
    flex: 1;
    color: rgba(255, 255, 255, 0.68);
    font-size: var(--font-xs);
  }

  .lightbox-status.error {
    color: #ffd0ca;
  }

  .lightbox-close {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    padding: 0;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.1);
    color: white;
    cursor: pointer;
  }

  .lightbox-close:hover {
    background: rgba(255, 255, 255, 0.18);
  }

  .lightbox-stage {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }

  .lightbox-stage img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    display: block;
    border-radius: var(--radius-s);
    box-shadow: var(--shadow-3);
  }

  .lightbox-empty {
    color: rgba(255, 255, 255, 0.72);
    font-size: var(--font-sm);
  }

  .lightbox-loading {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    border: 3px solid rgba(255, 255, 255, 0.22);
    border-top-color: white;
    animation: original-spin 0.9s linear infinite;
  }

  @keyframes original-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
