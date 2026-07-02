<script lang="ts">
  import { untrack } from "svelte";

  import { app } from "../app-state.svelte";
  import { albumBrowse, syncAlbums } from "../album-store.svelte";
  import { sectionMenu } from "../section-context-menu.svelte";
  import { restoreScrollPosition, saveScrollPosition } from "../view-state";
  import AlbumCard from "./AlbumCard.svelte";
  import AlbumReader from "./AlbumReader.svelte";

  let { active = true }: { active?: boolean } = $props();

  const sortedAlbums = $derived(
    albumBrowse.sortByCount
      ? albumBrowse.albums
      : [...albumBrowse.albums].sort((a, b) => (a.alias ?? a.key).localeCompare(b.alias ?? b.key)),
  );

  const openAlbum = $derived(
    albumBrowse.openAlbumKey
      ? (albumBrowse.albums.find(album => album.key === albumBrowse.openAlbumKey) ?? null)
      : null,
  );

  // 数据/别名变化时重载画册列表；缓存有效时切回秒开。
  $effect(() => {
    if (!active) {
      return;
    }
    void app.dataVersion;
    void sectionMenu.aliasVersion;
    untrack(() => syncAlbums());
  });

  let listEl = $state<HTMLDivElement | null>(null);

  // 切回时恢复画册列表滚动位置（卡片封面异步加载，内部按帧重试）
  let restored = false;
  $effect(() => {
    if (restored || !active || !listEl || albumBrowse.loading) {
      return;
    }
    restored = true;
    restoreScrollPosition(listEl, "albums");
  });

  function onScroll(): void {
    saveScrollPosition("albums", listEl?.scrollTop ?? 0);
  }
</script>

{#if openAlbum}
  <AlbumReader album={openAlbum} {active} onback={() => (albumBrowse.openAlbumKey = null)} />
{:else}
  <div class="album-browse" bind:this={listEl} onscroll={onScroll}>
    <div class="mode-bar">
      <span class="mode-label">排序：</span>
      <button
        type="button"
        class:is-active={albumBrowse.sortByCount}
        onclick={() => (albumBrowse.sortByCount = true)}
      >按数量</button>
      <button
        type="button"
        class:is-active={!albumBrowse.sortByCount}
        onclick={() => (albumBrowse.sortByCount = false)}
      >按名称</button>
    </div>

    {#if albumBrowse.loading}
      <div class="status"><p class="muted">正在加载画册…</p></div>
    {:else if albumBrowse.error}
      <div class="status"><p class="muted">加载失败：{albumBrowse.error}</p></div>
    {:else if albumBrowse.albums.length === 0}
      <div class="status"><p class="muted">还没有可成册的画师串（图片需含画师串）。</p></div>
    {:else}
      <div class="album-grid">
        {#each sortedAlbums as album (album.key)}
          <AlbumCard {album} onopen={() => (albumBrowse.openAlbumKey = album.key)} />
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
