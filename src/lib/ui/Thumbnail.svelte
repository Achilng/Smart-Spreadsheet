<script lang="ts">
  import { beginFileDrag } from "../stores/file-drag";
  import { isImageLoadCancelled } from "../images/image-loader";
  import { galleryPreviews } from "../images/progressive-images";
  import { thumbnails } from "../images/thumbnails";

  let {
    rowId,
    hasImage,
    alt,
    enhance = false,
    highPriority = false,
  }: {
    rowId: number;
    hasImage: boolean;
    alt: string;
    enhance?: boolean;
    highPriority?: boolean;
  } = $props();

  let url = $state<string | null>(null);
  let enhancedUrl = $state<string | null>(null);
  let enhancedReady = $state(false);
  let failed = $state(false);
  let failReason = $state<string | null>(null);
  let retryToken = $state(0);

  function revealEnhancedImage(event: Event): void {
    const image = event.currentTarget as HTMLImageElement;
    const expectedUrl = enhancedUrl;
    void image.decode().catch(() => {}).then(() => {
      requestAnimationFrame(() => {
        if (image.isConnected && enhancedUrl === expectedUrl) {
          enhancedReady = true;
        }
      });
    });
  }

  function retry(): void {
    retryToken += 1;
  }

  $effect(() => {
    void retryToken;
    url = null;
    enhancedUrl = null;
    enhancedReady = false;
    failed = false;
    failReason = null;
    const id = rowId;
    if (!hasImage) {
      return;
    }
    // 命中缓存时同步取值，避免已缓存的图闪一帧灰块
    const cachedThumb = thumbnails.cached(id);
    if (cachedThumb) {
      url = cachedThumb;
    }
    enhancedUrl = galleryPreviews.cached(id);
    if (cachedThumb) {
      return;
    }
    let cancelled = false;
    thumbnails.load(id).then(
      loaded => {
        if (!cancelled) {
          url = loaded;
        }
      },
      error => {
        // 取消（滚出视口/切视图/缓存重置）不算失败：滚回来会重新触发加载。
        if (!cancelled && !isImageLoadCancelled(error)) {
          failed = true;
          failReason = error instanceof Error ? error.message : String(error);
        }
      },
    );
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    const id = rowId;
    if (!hasImage || !enhance || enhancedUrl) {
      return;
    }
    let cancelled = false;
    galleryPreviews.load(id, highPriority).then(
      loaded => {
        if (!cancelled) {
          enhancedReady = false;
          enhancedUrl = loaded;
        }
      },
      () => {},
    );
    return () => {
      cancelled = true;
    };
  });
</script>

{#if url}
  <span class="thumbnail-stack">
    <img
      class="base"
      src={url}
      {alt}
      loading="lazy"
      draggable="false"
      onmousedown={(e) => { if (hasImage) beginFileDrag(e, rowId); }}
    />
    {#if enhancedUrl}
      <img
        class="enhanced"
        class:is-ready={enhancedReady}
        src={enhancedUrl}
        alt=""
        aria-hidden="true"
        decoding="async"
        draggable="false"
        onload={revealEnhancedImage}
        onmousedown={(e) => { if (hasImage) beginFileDrag(e, rowId); }}
      />
    {/if}
  </span>
{:else if !hasImage}
  <span class="note">无图</span>
{:else if failed}
  <button
    type="button"
    class="note retry"
    title={failReason ? `${failReason}（点击重试）` : "点击重试"}
    onclick={retry}
  >不可用 · 重试</button>
{:else}
  <span class="loading" aria-hidden="true"></span>
{/if}

<style>
  .thumbnail-stack {
    position: relative;
    display: block;
    width: 100%;
    height: 100%;
    overflow: hidden;
    border-radius: var(--radius-s);
  }

  img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
    border-radius: var(--radius-s);
  }

  .enhanced {
    visibility: hidden;
  }

  .enhanced.is-ready {
    visibility: visible;
  }

  .note {
    font-size: var(--font-xs);
    color: var(--text-3);
  }

  .retry {
    border: none;
    background: transparent;
    padding: 4px 8px;
    cursor: pointer;
    border-radius: var(--radius-s);
  }

  .retry:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .loading {
    display: block;
    width: 100%;
    height: 100%;
    border-radius: var(--radius-s);
    background: var(--surface-2);
  }
</style>
