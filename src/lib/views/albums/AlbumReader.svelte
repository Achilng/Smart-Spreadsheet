<script lang="ts">
  import {
    getDedupeClusterMembers,
    getRowPreview,
    type DedupeCluster,
    type RowRecord,
  } from "../../api";
  import { binaryBuffer } from "../../images/image-loader";
  import { saveAlbumIndex, savedAlbumIndex } from "../../stores/album-store.svelte";
  import { errorText, formatCount } from "../../stores/app-state.svelte";
  import { beginFileDrag } from "../../stores/file-drag";
  import { rowStore } from "../../stores/row-store.svelte";
  import Thumbnail from "../../ui/Thumbnail.svelte";

  let {
    album,
    active = true,
    onback,
  }: {
    album: DedupeCluster;
    active?: boolean;
    onback: () => void;
  } = $props();

  const PAGE = 200;
  let members = $state<RowRecord[]>([]);
  let totalCount = $state(0);
  let index = $state(0);
  let loading = $state(true);
  let loadingMore = $state(false);
  let error = $state<string | null>(null);

  let previewUrl = $state<string | null>(null);
  let previewError = $state<string | null>(null);

  const current = $derived(members[index] ?? null);

  void loadInitial();

  async function loadInitial(): Promise<void> {
    loading = true;
    error = null;
    try {
      const page = await fetchPage(0);
      members = page.rows;
      totalCount = page.totalCount;
      // 恢复上次阅读进度：补加载到目标页（数据可能已变化，越界时钳位）
      const target = Math.min(savedAlbumIndex(album.key), Math.max(0, totalCount - 1));
      while (members.length <= target && members.length < totalCount) {
        const more = await fetchPage(members.length);
        if (more.rows.length === 0) break;
        members = [...members, ...more.rows];
        totalCount = more.totalCount;
      }
      index = Math.max(0, Math.min(target, members.length - 1));
    } catch (e) {
      error = errorText(e);
    } finally {
      loading = false;
    }
  }

  function fetchPage(offset: number) {
    return getDedupeClusterMembers(
      "artists",
      album.key,
      [],
      "and",
      false,
      false,
      offset,
      PAGE,
    );
  }

  async function loadMore(): Promise<void> {
    if (loadingMore || members.length >= totalCount) return;
    loadingMore = true;
    try {
      const page = await fetchPage(members.length);
      members = [...members, ...page.rows];
      totalCount = page.totalCount;
    } catch (e) {
      error = errorText(e);
    } finally {
      loadingMore = false;
    }
  }

  async function next(): Promise<void> {
    if (index >= totalCount - 1) return;
    if (index >= members.length - 1) {
      await loadMore();
    }
    if (index < members.length - 1) index += 1;
  }

  function prev(): void {
    if (index > 0) index -= 1;
  }

  function jumpTo(target: number): void {
    index = target;
  }

  function hasImage(row: RowRecord): boolean {
    return Boolean(row.imagePath?.trim() || row.storedImagePath?.trim());
  }

  // 当前图同步到右侧详情面板，并记住本册阅读进度。
  $effect(() => {
    if (active && current) {
      rowStore.activeRow = current;
      saveAlbumIndex(album.key, index);
    }
  });

  // 胶片条上让当前图保持可见（含进度恢复后的首次定位）
  function keepVisible(node: HTMLElement, isCurrent: boolean) {
    const scroll = (flag: boolean) => {
      if (flag) node.scrollIntoView({ block: "nearest", inline: "center" });
    };
    scroll(isCurrent);
    return { update: scroll };
  }

  // 加载当前图大图预览，切图时撤销旧对象 URL。
  $effect(() => {
    previewUrl = null;
    previewError = null;
    if (!active) return;
    const row = current;
    if (!row) return;
    if (!hasImage(row)) {
      previewError = "无图";
      return;
    }
    let cancelled = false;
    let createdUrl: string | null = null;
    void getRowPreview(row.id).then(
      bytes => {
        const url = URL.createObjectURL(new Blob([binaryBuffer(bytes)], { type: "image/png" }));
        if (cancelled) {
          URL.revokeObjectURL(url);
          return;
        }
        createdUrl = url;
        previewUrl = url;
      },
      e => {
        if (!cancelled) previewError = errorText(e);
      },
    );
    return () => {
      cancelled = true;
      if (createdUrl) URL.revokeObjectURL(createdUrl);
    };
  });

  function onKeydown(event: KeyboardEvent): void {
    if (!active) {
      return;
    }
    const target = event.target;
    if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) {
      return;
    }
    if (event.key === "ArrowRight") {
      event.preventDefault();
      void next();
    } else if (event.key === "ArrowLeft") {
      event.preventDefault();
      prev();
    } else if (event.key === "Escape") {
      onback();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="reader">
  <div class="reader-bar">
    <button type="button" class="btn" onclick={onback}>← 返回画册</button>
    <span class="reader-title" title={album.key}>{album.alias ?? album.key}</span>
    <span class="reader-count">
      {totalCount > 0 ? `${index + 1} / ${formatCount(totalCount)}` : ""}
    </span>
  </div>

  {#if loading}
    <div class="stage"><p class="muted">加载中…</p></div>
  {:else if error}
    <div class="stage"><p class="muted">加载失败：{error}</p></div>
  {:else if members.length === 0}
    <div class="stage"><p class="muted">该画册没有图片。</p></div>
  {:else}
    <div class="stage">
      <button
        type="button"
        class="nav"
        onclick={prev}
        disabled={index === 0}
        aria-label="上一张"
      >‹</button>
      <div class="image-wrap">
        {#if previewUrl}
          <img
            src={previewUrl}
            alt={album.key}
            draggable="false"
            onmousedown={e => {
              if (current) beginFileDrag(e, current.id);
            }}
          />
        {:else if previewError}
          <span class="muted">{previewError}</span>
        {:else}
          <span class="muted">…</span>
        {/if}
      </div>
      <button
        type="button"
        class="nav"
        onclick={() => void next()}
        disabled={index >= totalCount - 1}
        aria-label="下一张"
      >›</button>
    </div>

    <div class="filmstrip">
      {#each members as member, i (member.id)}
        <button
          type="button"
          class="film"
          class:is-current={i === index}
          use:keepVisible={i === index}
          onclick={() => jumpTo(i)}
          title={`#${member.sourceOrdinal}`}
        >
          <Thumbnail
            rowId={member.id}
            hasImage={hasImage(member)}
            alt={`#${member.sourceOrdinal}`}
          />
        </button>
      {/each}
      {#if members.length < totalCount}
        <button
          type="button"
          class="film film-more"
          disabled={loadingMore}
          onclick={() => void loadMore()}
        >
          {loadingMore ? "…" : "更多"}
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .reader {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }

  .reader-bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    flex: none;
  }

  .reader-title {
    flex: 1;
    font-weight: 600;
    font-size: var(--font-base);
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .reader-count {
    font-size: var(--font-sm);
    color: var(--text-3);
    flex: none;
  }

  .stage {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 12px;
  }

  .image-wrap {
    flex: 1;
    min-width: 0;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }

  .image-wrap img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    display: block;
  }

  .nav {
    flex: none;
    width: 44px;
    height: 64px;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--surface);
    color: var(--text-2);
    font-size: 28px;
    line-height: 1;
    cursor: pointer;
  }

  .nav:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--text);
  }

  .nav:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .filmstrip {
    flex: none;
    display: flex;
    gap: 6px;
    padding: 8px 12px;
    overflow-x: auto;
    border-top: 1px solid var(--border);
    background: var(--surface);
  }

  .film {
    flex: none;
    width: 64px;
    height: 64px;
    padding: 0;
    border: 2px solid transparent;
    border-radius: var(--radius-s);
    background: var(--surface-2);
    overflow: hidden;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .film.is-current {
    border-color: var(--accent);
  }

  .film-more {
    font-size: var(--font-sm);
    color: var(--text-2);
  }
</style>
