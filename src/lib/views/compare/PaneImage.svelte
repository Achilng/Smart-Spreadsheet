<script lang="ts">
  import { detailPreviews, galleryPreviews } from "../../images/progressive-images";
  import { thumbnails } from "../../images/thumbnails";

  let {
    rowId,
    hasImage,
    alt,
    tier = "detail",
  }: {
    rowId: number;
    hasImage: boolean;
    alt: string;
    tier?: "gallery" | "detail";
  } = $props();

  let displayUrl = $state<string | null>(null);
  let loadFailed = $state(false);

  // 渐进加载：256px 缩略图占位，2048px 详情层到达后替换（详情层优先加载）。
  $effect(() => {
    displayUrl = null;
    loadFailed = false;
    const id = rowId;
    if (!hasImage) {
      return;
    }

    let cancelled = false;
    let failedLoads = 0;
    const markFailure = () => {
      failedLoads += 1;
      if (!cancelled && failedLoads === 2 && displayUrl === null) {
        loadFailed = true;
      }
    };
    const cachedThumb = thumbnails.cached(id);
    if (cachedThumb) {
      displayUrl = cachedThumb;
    } else {
      void thumbnails.load(id).then(
        loaded => {
          // 详情图可能先返回；不要让迟到的缩略图把高清层覆盖回去。
          if (!cancelled && displayUrl === null) {
            displayUrl = loaded;
          }
        },
        markFailure,
      );
    }

    const previews = tier === "gallery" ? galleryPreviews : detailPreviews;
    void previews.load(id, true).then(
      loaded => {
        if (!cancelled) {
          displayUrl = loaded;
          loadFailed = false;
        }
      },
      () => {
        // 已有缩略图时仍可正常展示；只有两层都失败才进入失败态。
        if (cachedThumb) return;
        markFailure();
      },
    );
    return () => {
      cancelled = true;
    };
  });
</script>

{#if displayUrl}
  <img src={displayUrl} {alt} draggable="false" decoding="async" />
{:else if loadFailed}
  <span class="media-failed">图片加载失败</span>
{:else}
  <span class="media-loading" aria-hidden="true"></span>
{/if}

<style>
  img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    display: block;
  }

  .media-loading {
    width: 44px;
    height: 44px;
    border-radius: 50%;
    border: 3px solid var(--surface-2);
    border-top-color: var(--text-4);
    animation: pane-spin 0.9s linear infinite;
  }

  .media-failed {
    color: var(--text-3);
    font-size: var(--font-sm);
  }

  @keyframes pane-spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) { .media-loading { animation: none; } }
</style>
