<script lang="ts">
  import { getDedupeClusterMembers, type DedupeCluster } from "../../api";
  import { formatCount } from "../../stores/app-state.svelte";
  import { thumbnails } from "../../images/thumbnails";

  let { album, onopen }: { album: DedupeCluster; onopen: () => void } = $props();

  let el = $state<HTMLButtonElement | null>(null);
  let thumbUrl = $state<string | null>(null);
  let noCover = $state(false);
  let fetched = false;

  // 仅在卡片进入可视区时才取首张成员作封面，避免上千画册一次性请求。
  $effect(() => {
    const node = el;
    if (!node) return;
    const observer = new IntersectionObserver(entries => {
      if (entries.some(entry => entry.isIntersecting) && !fetched) {
        fetched = true;
        observer.disconnect();
        void loadCover();
      }
    });
    observer.observe(node);
    return () => observer.disconnect();
  });

  async function loadCover(): Promise<void> {
    try {
      const page = await getDedupeClusterMembers(
        "artists",
        album.key,
        [],
        "and",
        false,
        false,
        0,
        1,
      );
      const first = page.rows[0];
      const hasImage =
        first && Boolean(first.imagePath?.trim() || first.storedImagePath?.trim());
      if (!first || !hasImage) {
        noCover = true;
        return;
      }
      thumbUrl = await thumbnails.load(first.id);
    } catch {
      noCover = true;
    }
  }
</script>

<button type="button" class="album-card" bind:this={el} onclick={onopen} title={album.key}>
  <div class="cover">
    {#if thumbUrl}
      <img src={thumbUrl} alt={album.alias ?? album.key} loading="lazy" draggable="false" />
    {:else if noCover}
      <span class="cover-note faint">无封面</span>
    {:else}
      <span class="cover-note faint">…</span>
    {/if}
    <span class="count-badge">{formatCount(album.memberCount)}</span>
  </div>
  <span class="album-name">{album.alias ?? album.key}</span>
</button>

<style>
  .album-card {
    display: flex;
    flex-direction: column;
    width: 160px;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--surface);
    overflow: hidden;
    cursor: pointer;
    padding: 0;
    transition: border-color 0.1s ease;
  }

  .album-card:hover {
    border-color: var(--border-strong);
  }

  .cover {
    position: relative;
    width: 100%;
    height: 160px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--surface-2);
    overflow: hidden;
  }

  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .cover-note {
    font-size: 11px;
  }

  .count-badge {
    position: absolute;
    right: 6px;
    bottom: 6px;
    padding: 1px 7px;
    font-size: 11px;
    color: #fff;
    background: rgb(0 0 0 / 60%);
    border-radius: 10px;
  }

  .album-name {
    display: block;
    padding: 6px 8px;
    font-size: 12px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    text-align: center;
  }
</style>
