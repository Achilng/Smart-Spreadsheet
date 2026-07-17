<script lang="ts">
  import { beginFileDrag } from "../stores/file-drag";
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

  $effect(() => {
    url = null;
    enhancedUrl = null;
    enhancedReady = false;
    failed = false;
    const id = rowId;
    if (!hasImage) {
      return;
    }
    let cancelled = false;
    thumbnails.load(id).then(
      loaded => {
        if (!cancelled) {
          url = loaded;
        }
      },
      () => {
        if (!cancelled) {
          failed = true;
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
      class:is-replaced={enhancedReady}
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
        onload={() => (enhancedReady = true)}
        onmousedown={(e) => { if (hasImage) beginFileDrag(e, rowId); }}
      />
    {/if}
  </span>
{:else if !hasImage}
  <span class="note">无图</span>
{:else if failed}
  <span class="note">不可用</span>
{:else}
  <span class="loading shimmer" aria-hidden="true"></span>
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

  .base {
    opacity: 1;
    transition: opacity 180ms ease;
  }

  .base.is-replaced {
    opacity: 0;
  }

  .enhanced {
    opacity: 0;
    transition: opacity 180ms ease;
  }

  .enhanced.is-ready {
    opacity: 1;
  }

  .note {
    font-size: var(--font-xs);
    color: var(--text-3);
  }

  .loading {
    display: block;
    width: 100%;
    height: 100%;
    border-radius: var(--radius-s);
  }
</style>
