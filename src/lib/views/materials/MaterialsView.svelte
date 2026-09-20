<script lang="ts">
  import { onDestroy, onMount, untrack } from "svelte";
  import { confirm, open } from "@tauri-apps/plugin-dialog";
  import { listMaterials, materialTagCounts, deleteMaterial, materialImage, materialVersionImage, type Material } from "../../api/materials";
  import type { TagSummary } from "../../api/tags";
  import { app, errorText, setNotice } from "../../stores/app-state.svelte";
  import { loadTags, tagStore } from "../../stores/tag-store.svelte";
  import { ImageLoader } from "../../images/image-loader";
  import TagFilterSidebar from "../../ui/TagFilterSidebar.svelte";
  import { PagedCollection } from "../../utils/paged-collection";
  import { materialBrowser } from "../../stores/material-browser.svelte";
  import { MATERIAL_IMAGE_EXTENSIONS, isMaterialImagePath } from "../../utils/materials";
  import { anyModalOpen } from "../../stores/modal-layer.svelte";
  import GalleryTile from "../gallery/GalleryTile.svelte";
  import GalleryViewport from "../gallery/GalleryViewport.svelte";
  import { galleryLayout, galleryCellPosition, galleryVisibleIndices } from "../gallery/gallery-layout";
  import SizeSlider from "../shell/SizeSlider.svelte";
  import Thumbnail from "../../ui/Thumbnail.svelte";
  import MaterialDetailPanel from "./MaterialDetailPanel.svelte";
  import DetailSidebar from "../../ui/DetailSidebar.svelte";
  import MaterialVersionMenu from "./MaterialVersionMenu.svelte";
  import MaterialEditor from "./MaterialEditor.svelte";

  let { active }: { active: boolean } = $props();
  const thumbnails = new ImageLoader(id => materialImage(id, true), 4, 144, "image/png");
  const versionCovers = new ImageLoader(materialVersionImage, 2, 16, "image/png");
  const covers = new ImageLoader(id => materialImage(id, false), 2, 8, "image/png");
  const PAGE_SIZE = 48;
  let pagesVersion = $state(0);
  const pages = new PagedCollection<Material>(PAGE_SIZE, () => pagesVersion++);
  const total = $derived.by(() => { void pagesVersion; return pages.total; });
  const loading = $derived.by(() => { void pagesVersion; return !pages.ready && !pages.errors.size; });
  const error = $derived.by(() => { void pagesVersion; return pages.errors.size ? errorText(pages.errors.values().next().value) : ""; });
  let tagError = $state("");
  let tags = $state<TagSummary[]>([]);
  let selected = $state<Material | null>(null);
  let detailOpen = $state(true);
  let tagSearch = $state("");
  let selectedTags = $state<string[]>([]);
  let untagged = $state(false);
  let revision = $state(0);
  let editorOpen = $state(false);
  let editing = $state<Material | null>(null);
  let editingVersion = $state<number | undefined>();
  let pendingPaths = $state<string[]>([]);
  let editorKey = $state(0);
  let request = 0;
  let lastDirectory: string | null | undefined;
  let lastFilter = "";
  let keepSavedSelection = false;

  let viewport = $state<HTMLDivElement | null>(null);
  let measuredWidth = $state(0);
  let measuredHeight = $state(0);
  let viewportWidth = $state(0);
  let viewportHeight = $state(0);
  let scrollTop = $state(0);
  $effect(() => {
    if (measuredWidth > 0) viewportWidth = measuredWidth;
    if (measuredHeight > 0) viewportHeight = measuredHeight;
  });
  const layout = $derived(galleryLayout(viewportWidth, app.galleryCardSize, total));
  const cells = $derived.by(() => {
    void pagesVersion;
    if (!active || viewportWidth <= 0) return [];
    return galleryVisibleIndices(layout, scrollTop, viewportHeight, total).map(index => ({
      index, item: pages.get(index), ...galleryCellPosition(index, layout),
    }));
  });
  const entries = $derived([...tags, ...selectedTags.filter(name => !tags.some(tag => tag.name === name)).map(name => ({ name, rowCount: 0 }))]);

  $effect(() => {
    const directory = app.snapshot?.dataDirectory;
    if (directory !== lastDirectory) {
      lastDirectory = directory;
      untrack(() => {
        selected = null; materialBrowser.search = ""; selectedTags = []; untagged = false;
        editorOpen = false; pendingPaths = []; thumbnails.clear(); covers.clear(); versionCovers.clear(); revision++;
      });
    }
  });
  $effect(() => {
    void app.dataVersion; void tagStore.list; void revision;
    const query = materialBrowser.search, filter = [...selectedTags], noTags = untagged;
    // Reset immediately so a slow response cannot enter a newer search result.
    const token = ++request;
    untrack(() => {
      const key = JSON.stringify([query, filter, noTags]);
      if (key !== lastFilter && !keepSavedSelection) selected = null;
      lastFilter = key;
      keepSavedSelection = false;
      scrollTop = 0;
      if (viewport) viewport.scrollTop = 0;
      pages.reset(offset => listMaterials(query, filter, noTags, offset));
      tagError = "";
    });
    const timer = setTimeout(() => {
      void pages.load(0);
      void materialTagCounts().then(counts => { if (token === request) tags = counts; })
        .catch(cause => { if (token === request) tagError = errorText(cause); });
    }, 180);
    return () => { clearTimeout(timer); request++; };
  });
  $effect(() => {
    const visible = cells;
    const missing = new Set<number>();
    const ids = new Set<number>();
    for (const cell of visible) {
      if (cell.item) ids.add(cell.item.id);
      else missing.add(Math.floor(cell.index / PAGE_SIZE));
    }
    untrack(() => {
      thumbnails.retain(ids);
      for (const page of missing) void pages.load(page);
    });
  });
  // Keep the last scroll offset while the persistent view is hidden.
  $effect(() => {
    if (!active || !viewport || measuredWidth <= 0 || measuredHeight <= 0) return;
    void layout.spacerHeight;
    viewport.scrollTop = untrack(() => scrollTop);
  });
  function onScroll() { if (active) scrollTop = viewport?.scrollTop ?? 0; }
  function filterTag(name: string) { selectedTags = selectedTags.includes(name) ? selectedTags.filter(t => t !== name) : [...selectedTags, name]; untagged = false; }
  function clearFilter() { selectedTags = []; untagged = false; materialBrowser.search = ""; }
  function beginImport(paths: string[]) {
    if (!active || editorOpen || app.busy || anyModalOpen()) return;
    const valid = paths.filter(isMaterialImagePath);
    if (!valid.length) { setNotice({ tone: "error", text: "请拖入 PNG、JPG、WebP 等图片文件。" }); return; }
    if (valid.length !== paths.length) setNotice({ tone: "error", text: `已跳过 ${paths.length - valid.length} 个非图片项目。` });
    pendingPaths = valid; editing = null; editorKey++; editorOpen = true;
  }
  async function chooseImages() {
    try {
      const result = await open({ multiple: true, directory: false, title: "导入素材展示图", filters: [{ name: "图片", extensions: MATERIAL_IMAGE_EXTENSIONS }] });
      if (result) beginImport(typeof result === "string" ? [result] : result);
    } catch (cause) { setNotice({ tone: "error", text: errorText(cause) }); }
  }
  function create() { editing = null; editingVersion = undefined; pendingPaths = []; editorKey++; editorOpen = true; }
  function editItem(item: Material) { selected = item; edit(); }
  function edit(versionId?: number) { if (selected) { editing = selected; editingVersion = versionId; pendingPaths = []; editorKey++; editorOpen = true; } }
  function closeEditor() { editorOpen = false; editing = null; pendingPaths = []; }
  function saved(item: Material) {
    thumbnails.clear(); covers.clear(); versionCovers.clear(); revision++;
    keepSavedSelection = true; selected = item; clearFilter();
    void loadTags();
    setNotice({ tone: "success", text: `素材「${item.title}」已保存。` });
    if (!editing && pendingPaths.length > 1) { pendingPaths = pendingPaths.slice(1); editorKey++; }
    else closeEditor();
  }
  async function copy(item: Material) {
    const text = item.versions[0]?.text ?? item.text;
    if (!text) { setNotice({ tone: "error", text: "这份素材还没有文本，请先编辑内容。" }); return; }
    try { await navigator.clipboard.writeText(text); setNotice({ tone: "success", text: `已复制「${item.title}」的文本。` }); }
    catch (cause) { setNotice({ tone: "error", text: `复制失败：${errorText(cause)}` }); }
  }
  async function remove() {
    const item = selected;
    if (!item || !(await confirm(`删除素材「${item.title}」及其展示图和文本？原始图片不会被修改。`, { title: "删除素材", kind: "warning", okLabel: "删除", cancelLabel: "取消" }))) return;
    try { await deleteMaterial(item.id); selected = null; thumbnails.clear(); covers.clear(); versionCovers.clear(); revision++; setNotice({ tone: "success", text: "素材已删除。" }); }
    catch (cause) { setNotice({ tone: "error", text: errorText(cause) }); }
  }
  onMount(() => {
    const handler = (event: Event) => beginImport((event as CustomEvent<string[]>).detail);
    window.addEventListener("material-path-drop", handler);
    return () => window.removeEventListener("material-path-drop", handler);
  });
  onDestroy(() => { request++; pages.dispose(); thumbnails.dispose(); covers.dispose(); versionCovers.dispose(); });
</script>

<section class="materials" inert={editorOpen}>
  <aside class="filter-sidebar">
    <TagFilterSidebar {entries} activeTags={selectedTags} ontoggle={filterTag} searchable bind:query={tagSearch}
      summary={selectedTags.length || untagged ? `${selectedTags.length} 个 Tag · ${Number(untagged)} 个条件生效` : "未启用筛选"}
      modeLabel="同时匹配" error={tagError} emptyText="还没有 Tag。编辑素材时可以添加。">
      {#snippet filters()}
        <div class="f-group" role="group" aria-label="素材显示条件">
          <div class="f-head">显示</div>
          <label class="check-row" class:on={untagged}>
            <input type="checkbox" checked={untagged} onchange={() => { untagged = !untagged; selectedTags = []; }} />
            <span class="cbox" aria-hidden="true"></span>无 Tag 素材
          </label>
          {#if selectedTags.length || untagged || materialBrowser.search}<button type="button" class="mode-link" onclick={clearFilter}>清除全部筛选</button>{/if}
        </div>
      {/snippet}
    </TagFilterSidebar>
  </aside>
  <main>
    <header><div><h1>素材 <small>{total}</small></h1></div><div class="actions"><SizeSlider /><button class="btn" onclick={() => void chooseImages()}>导入图片</button><button class="btn btn-primary" onclick={create}>新建素材</button></div></header>
    {#if selectedTags.length}<p class="filter-summary">Tag：{selectedTags.join("、")}</p>{/if}
    {#if error}<div class="load-error" role="alert">{error}<button class="btn" onclick={() => pages.retry()}>重试</button></div>{/if}
    <GalleryViewport bind:viewport bind:measuredWidth bind:measuredHeight onscroll={onScroll} spacerHeight={total ? layout.spacerHeight : undefined} busy={loading}>
      {#if !total}
        <div class="empty"><h2>{loading ? "正在读取素材…" : error ? "素材读取失败" : materialBrowser.search || selectedTags.length || untagged ? "没有匹配的素材" : "收藏你的第一份素材"}</h2>
          {#if !loading && !error}<p>新建素材可从图库选图，也可以将本地图片拖到这里导入。</p>{/if}
        </div>
      {:else}
        {#each cells as cell (`${cell.index}-${revision}`)}
          {@const item = cell.item}
          <GalleryTile x={cell.x} y={cell.y} width={layout.cardWidth} imageHeight={layout.imageHeight}
            skeleton={!item} title={item?.title ?? ""} titleAlign="center" tags={item?.tags ?? []} isActive={!!item && selected?.id === item.id}
            onclick={() => { if (item) selected = item; }} ondblclick={() => { if (item) void copy(item); }}>
            {#snippet overlayControls()}{#if item && item.versions.length > 1}<MaterialVersionMenu material={item} onmanage={() => editItem(item)} />{/if}{/snippet}
            {#snippet image()}{#if item}<Thumbnail rowId={item.id} loader={thumbnails} previewLoader={null} allowFileDrag={false} hasImage={true} alt={item.title} />{/if}{/snippet}
          </GalleryTile>
        {/each}
      {/if}
    </GalleryViewport>
  </main>
  <DetailSidebar open={detailOpen} onopen={() => detailOpen = true}>
    <MaterialDetailPanel material={selected} {revision} active={active && !editorOpen} loader={covers} versionLoader={versionCovers}
      onedit={edit} ondelete={() => void remove()} oncollapse={() => detailOpen = false} />
  </DetailSidebar>
</section>
{#if editorOpen}{#key editorKey}<MaterialEditor material={editing} versionId={editingVersion} path={pendingPaths[0] ?? null} remaining={Math.max(0, pendingPaths.length - 1)} loader={covers} versionLoader={versionCovers} onsaved={saved} onclose={closeEditor} />{/key}{/if}

<style>
  .materials { width: 100%; height: 100%; display: flex; min-height: 0; color: var(--text); }
  .filter-sidebar { width: 240px; flex: none; min-height: 0; background: var(--surface); border-right: 1px solid var(--border); }
  h1,h2,p { margin: 0; } h1 { font-size: 25px; } h1 small { font-size: 14px; color: var(--text-3); font-weight: 400; }
  main { flex: 1; display: flex; flex-direction: column; min-height: 0; min-width: 0; overflow: hidden; }
  header { display: flex; justify-content: space-between; gap: 14px; align-items: center; padding: 16px; }
  .actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; } .filter-summary { padding: 0 16px 8px; color: var(--accent); font-size: var(--font-sm); }
  .empty { height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 15px; color: var(--text-3); text-align: center; } .empty h2 { font-size: 20px; color: var(--text-2); }
  .load-error { display: flex; align-items: center; gap: 12px; padding: 8px 16px; color: var(--danger); font-size: var(--font-sm); }
  @media (max-width: 1240px) { .filter-sidebar { width: 210px; } }
  @media (max-width: 1080px) { .filter-sidebar { width: 190px; } header { align-items: start; flex-direction: column; } }
</style>
