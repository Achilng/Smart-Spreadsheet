<script lang="ts">
  import { thumbnails } from "../thumbnails";

  let { rowId, hasImage, alt }: { rowId: number; hasImage: boolean; alt: string } = $props();

  let url = $state<string | null>(null);
  let failed = $state(false);

  $effect(() => {
    url = null;
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
</script>

{#if url}
  <img src={url} {alt} loading="lazy" />
{:else if !hasImage}
  <span class="note">无图</span>
{:else if failed}
  <span class="note">不可用</span>
{:else}
  <span class="note">…</span>
{/if}

<style>
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
    border-radius: 4px;
  }

  .note {
    font-size: 11px;
    color: var(--text-3);
  }
</style>
