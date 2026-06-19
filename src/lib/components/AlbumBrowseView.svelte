<script lang="ts">
  import { listArtistAlbums, type DedupeCluster } from "../../api";
  import { app, errorText } from "../app-state.svelte";
  import AlbumCard from "./AlbumCard.svelte";
  import AlbumReader from "./AlbumReader.svelte";

  let albums = $state<DedupeCluster[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let openAlbum = $state<DedupeCluster | null>(null);
  let sortByCount = $state(true);

  const sortedAlbums = $derived(
    sortByCount
      ? albums
      : [...albums].sort((a, b) => (a.alias ?? a.key).localeCompare(b.alias ?? b.key)),
  );

  // 初次挂载与数据变化（导入/删除）时重载画册列表。
  $effect(() => {
    void app.dataVersion;
    void load();
  });

  async function load(): Promise<void> {
    loading = true;
    error = null;
    try {
      albums = await listArtistAlbums();
      // 列表刷新后，若当前打开的画册已不存在则退回列表。
      if (openAlbum && !albums.some(album => album.key === openAlbum?.key)) {
        openAlbum = null;
      }
    } catch (e) {
      error = errorText(e);
      albums = [];
    } finally {
      loading = false;
    }
  }
</script>

{#if openAlbum}
  <AlbumReader album={openAlbum} onback={() => (openAlbum = null)} />
{:else}
  <div class="album-browse">
    <div class="mode-bar">
      <span class="mode-label">排序：</span>
      <button
        type="button"
        class:is-active={sortByCount}
        onclick={() => (sortByCount = true)}
      >按数量</button>
      <button
        type="button"
        class:is-active={!sortByCount}
        onclick={() => (sortByCount = false)}
      >按名称</button>
    </div>

    {#if loading}
      <div class="status"><p class="muted">正在加载画册…</p></div>
    {:else if error}
      <div class="status"><p class="muted">加载失败：{error}</p></div>
    {:else if albums.length === 0}
      <div class="status"><p class="muted">还没有可成册的画师串（图片需含画师串）。</p></div>
    {:else}
      <div class="album-grid">
        {#each sortedAlbums as album (album.key)}
          <AlbumCard {album} onopen={() => (openAlbum = album)} />
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .album-browse {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }

  .mode-bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }

  .mode-label {
    font-size: 13px;
    color: var(--text-2);
    margin-right: 4px;
  }

  .mode-bar button {
    border: 1px solid var(--border);
    background: transparent;
    border-radius: var(--radius-s);
    padding: 4px 12px;
    font-size: 12px;
    color: var(--text-2);
    cursor: pointer;
  }

  .mode-bar button:hover {
    background: var(--surface-2);
  }

  .mode-bar button.is-active {
    background: var(--surface-2);
    color: var(--text);
    border-color: var(--border-strong);
  }

  .status {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .album-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    padding: 16px;
    align-content: flex-start;
  }
</style>
