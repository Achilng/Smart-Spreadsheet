<script lang="ts">
  import { onDestroy } from "svelte";
  import { queryRows, getRowThumbnail, type RowRecord } from "../../api/rows";
  import { ImageLoader } from "../../images/image-loader";
  import { tagStore } from "../../stores/tag-store.svelte";
  import { errorText } from "../../stores/app-state.svelte";
  import MaterialImage from "./MaterialImage.svelte";

  let { busy, error, onchoose, oncancel }: {
    busy: boolean; error: string; onchoose: (rowId: number) => void; oncancel: () => void;
  } = $props();
  const loader = new ImageLoader(getRowThumbnail, 4, 96, "image/png");
  let search = $state("");
  let tag = $state("");
  let offset = $state(0);
  let rows = $state<RowRecord[]>([]);
  let total = $state(0);
  let selected = $state<number | null>(null);
  let loading = $state(true);
  let queryError = $state("");
  let revision = $state(0);
  let request = 0;
  const name = (row: RowRecord) => (row.imagePath ?? row.storedImagePath)?.split(/[\\/]/).pop() || `图片 #${row.id}`;

  $effect(() => {
    const text = search, filter = tag, start = offset;
    void revision;
    const token = ++request;
    loading = true; selected = null; rows = []; queryError = "";
    const timer = setTimeout(async () => {
      try {
        const page = await queryRows({ offset: start, limit: 48, search: text, tags: filter ? [filter] : [],
          tagMode: "and", dedupe: "none", singleArtistOnly: false, artistFilter: "", hasVibe: false,
          untaggedOnly: false, filters: [], groupView: false, hideGrouped: false, sort: "timeDesc" });
        if (token !== request) return;
        rows = page.rows; total = page.totalCount;
      } catch (cause) { if (token === request) queryError = errorText(cause); }
      finally { if (token === request) loading = false; }
    }, 180);
    return () => { clearTimeout(timer); request++; };
  });
  onDestroy(() => { request++; loader.dispose(); });
</script>

<section class="picker">
  <header><h3>从图库选择展示图</h3><p>选择一张图片后，可继续选择元数据和编辑素材内容。</p></header>
  <div class="filters">
    <input aria-label="搜索图库图片" placeholder="搜索提示词、画师或备注…" value={search} disabled={busy} oninput={e => { search = e.currentTarget.value; offset = 0; }} />
    <select aria-label="按图库 Tag 筛选" value={tag} disabled={busy} onchange={e => { tag = e.currentTarget.value; offset = 0; }}>
      <option value="">全部 Tag</option>
      {#each tagStore.list as item (item.name)}<option value={item.name}>{item.name}</option>{/each}
    </select>
  </div>
  <div class="grid" aria-busy={loading || busy}>
    {#each rows as row (row.id)}
      <button class="card" class:selected={selected === row.id} aria-pressed={selected === row.id} disabled={busy} onclick={() => selected = row.id} title={name(row)}>
        <div class="image"><MaterialImage id={row.id} {loader} alt={name(row)} /></div>
        <span>{name(row)}</span><small>{row.tags.join(" · ") || "无 Tag"}</small>
      </button>
    {:else}<p class="empty">{loading ? "正在读取图库…" : queryError ? "图库读取失败" : search || tag ? "没有匹配的图片" : "图库中还没有图片，请先向图库导入图片，或从本地文件选择展示图。"}</p>{/each}
  </div>
  {#if queryError}<p class="error" role="alert">{queryError} <button class="btn" onclick={() => revision++}>重试</button></p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <footer>
    <div class="pages"><span>{total} 张</span><button class="btn" disabled={busy || loading || offset === 0} onclick={() => offset -= 48}>上一页</button><span>{Math.floor(offset / 48) + 1} / {Math.max(1, Math.ceil(total / 48))}</span><button class="btn" disabled={busy || loading || offset + 48 >= total} onclick={() => offset += 48}>下一页</button></div>
    <div class="actions"><button class="btn" disabled={busy} onclick={oncancel}>返回编辑</button><button class="btn btn-primary" disabled={busy || loading || selected === null} onclick={() => selected !== null && onchoose(selected)}>{busy ? "正在读取图片…" : "使用这张图片"}</button></div>
  </footer>
</section>

<style>
  .picker { display: flex; flex-direction: column; gap: 16px; min-height: 0; }
  h3, p { margin: 0; } header p { margin-top: 8px; color: var(--text-3); font-size: var(--font-sm); }
  .filters, footer, .pages, .actions { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  input, select { padding: 9px 11px; border: 1px solid var(--border); border-radius: 7px; background: var(--surface); color: var(--text); min-width: 0; }
  input { flex: 1; } select { max-width: 180px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(135px, 1fr)); gap: 12px; height: min(420px, 45vh); overflow-y: auto; align-content: start; padding: 3px; }
  .card { min-width: 0; padding: 0 0 9px; border: 1px solid var(--border); background: var(--surface); color: var(--text); border-radius: 8px; overflow: hidden; text-align: left; }
  .card.selected { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-soft); }
  .image { height: 145px; background: var(--surface-2); } .card span, .card small { display: block; padding: 6px 8px 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .card small, .pages { color: var(--text-3); font-size: var(--font-sm); } .empty { grid-column: 1 / -1; padding: 45px 15px; text-align: center; color: var(--text-3); }
  footer { justify-content: space-between; } .error { color: var(--danger); font-size: var(--font-sm); }
</style>
