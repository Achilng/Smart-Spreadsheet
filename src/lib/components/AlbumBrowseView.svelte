<script lang="ts">
  import { listArtistAlbums, type DedupeCluster } from "../../api";
  import { app, errorText } from "../app-state.svelte";
  import AlbumCard from "./AlbumCard.svelte";
  import AlbumReader from "./AlbumReader.svelte";

  let albums = $state<DedupeCluster[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let openAlbum = $state<DedupeCluster | null>(null);

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
    {#if loading}
      <div class="status"><p class="muted">正在加载画册…</p></div>
    {:else if error}
      <div class="status"><p class="muted">加载失败：{error}</p></div>
    {:else if albums.length === 0}
      <div class="status"><p class="muted">还没有可成册的画师串（图片需含画师串）。</p></div>
    {:else}
      <div class="album-grid">
        {#each albums as album (album.key)}
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
