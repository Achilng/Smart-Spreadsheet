<script lang="ts">
  import { detailPreviews, galleryPreviews } from "../../images/progressive-images";
  import { thumbnails } from "../../images/thumbnails";

  /**
   * 对比窗口的大图：缩略图（缓存命中即显）→ 1024 高清层 → 2048 详情预览
   * 逐级替换，与主窗口详情面板同一套加载器。
   */
  let {
    rowId,
    hasImage,
    alt,
  }: {
    rowId: number;
    hasImage: boolean;
    alt: string;
  } = $props();

  let thumbUrl = $state<string | null>(null);
  let galleryUrl = $state<string | null>(null);
  let previewUrl = $state<string | null>(null);
  let failed = $state(false);

  $effect(() => {
    void rowId;
    thumbUrl = null;
    galleryUrl = null;
    previewUrl = null;
    failed = false;
    if (!hasImage) {
      detailPreviews.retain(new Set());
      return;
    }
    const id = rowId;
    detailPreviews.retain(new Set([id]));
    let cancelled = false;
    galleryUrl = galleryPreviews.cached(id);
    void thumbnails.load(id).then(
      url => {
        if (!cancelled) thumbUrl = url;
      },
      () => {},
    );
    void detailPreviews.load(id, true).then(
      url => {
        if (!cancelled) previewUrl = url;
      },
      () => {
        if (!cancelled) failed = !galleryUrl && !thumbUrl;
      },
    );
    return () => {
      cancelled = true;
      detailPreviews.retain(new Set());
    };
  });

  const displayUrl = $derived(previewUrl ?? galleryUrl ?? thumbUrl);
</script>

{#if displayUrl}
  <img class="large-image" src={displayUrl} {alt} draggable="false" />
{:else if !hasImage}
  <div class="placeholder">无图</div>
{:else if failed}
  <div class="placeholder">图片加载失败</div>
{:else}
  <div class="placeholder loading-placeholder" aria-hidden="true"></div>
{/if}

<style>
  .large-image {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
    border-radius: var(--radius-m);
  }

  .placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    min-height: 160px;
    border-radius: var(--radius-m);
    background: var(--surface-2);
    color: var(--text-3);
    font-size: var(--font-sm);
  }

  .loading-placeholder {
    animation: large-image-in var(--motion-base) var(--ease-responsive);
  }

  @keyframes large-image-in {
    from {
      opacity: 0;
    }
  }
</style>
