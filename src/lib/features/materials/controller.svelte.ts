import { errorText } from "../../utils/format";
import { setNotice } from "../../stores/notices.svelte";
import { libraryState } from "../../stores/library-state.svelte";
import { workspaceState } from "../../stores/workspace-state.svelte";
import { taskState } from "../../stores/task-state.svelte";
import { onDestroy, onMount, untrack } from "svelte";
import { open } from "@tauri-apps/plugin-dialog";
import {
  listMaterials,
  materialTagCounts,
  deleteMaterial,
  materialImage,
  materialVersionImage,
  type Material,
} from "../../api/materials";
import type { TagSummary } from "../../api/tags";
import { loadTags, tagStore } from "../../stores/tag-store.svelte";
import { ImageLoader } from "../../images/image-loader";
import { PagedCollection } from "../../utils/paged-collection";
import { materialBrowser } from "../../stores/material-browser.svelte";
import { MATERIAL_IMAGE_EXTENSIONS, isMaterialImagePath } from "../../utils/materials";
import { anyModalOpen } from "../../stores/modal-layer.svelte";
import { galleryLayout, galleryCellPosition, galleryVisibleIndices } from "../../images/gallery-layout";


/** Per-instance state and operations for the MaterialsView view. */
export function createMaterialsController(isActive: () => boolean) {
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
  let contextMenu = $state<{ item: Material; x: number; y: number } | null>(null);
  let deleting = $state(false);
  let pendingDelete = $state<{ item: Material; directory: string | null | undefined } | null>(null);
  let deleteError = $state<string | null>(null);
  let detailOpen = $state(true);
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
  const layout = $derived(galleryLayout(viewportWidth, workspaceState.galleryCardSize, total));
  const cells = $derived.by(() => {
    void pagesVersion;
    if (!isActive() || viewportWidth <= 0) return [];
    return galleryVisibleIndices(layout, scrollTop, viewportHeight, total).map(index => ({
      index, item: pages.get(index), ...galleryCellPosition(index, layout),
    }));
  });
  const entries = $derived([...tags, ...selectedTags.filter(name => !tags.some(tag => tag.name === name)).map(name => ({ name, rowCount: 0 }))]);

  $effect(() => {
    const directory = libraryState.snapshot?.dataDirectory;
    if (directory !== lastDirectory) {
      lastDirectory = directory;
      untrack(() => {
        selected = null; materialBrowser.search = ""; selectedTags = []; untagged = false;
        pendingDelete = null; deleteError = null;
        editorOpen = false; pendingPaths = []; thumbnails.clear(); covers.clear(); versionCovers.clear(); revision++;
      });
    }
  });
  $effect(() => {
    void libraryState.dataVersion; void tagStore.list; void revision;
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
    if (!isActive() || !viewport || measuredWidth <= 0 || measuredHeight <= 0) return;
    void layout.spacerHeight;
    viewport.scrollTop = untrack(() => scrollTop);
  });
  $effect(() => {
    void isActive(); void editorOpen; void taskState.busy; void libraryState.snapshot?.dataDirectory; void revision;
    void materialBrowser.search; void selectedTags; void untagged;
    untrack(() => contextMenu = null);
  });
  function showContextMenu(event: MouseEvent, item: Material) {
    event.preventDefault(); event.stopPropagation();
    if (!isActive() || editorOpen || taskState.busy || deleting || (event.target instanceof Element && event.target.closest('[role="menu"]'))) return;
    selected = item;
    contextMenu = { item, x: event.clientX, y: event.clientY };
  }
  function deleteFromMenu() {
    const item = contextMenu?.item;
    contextMenu = null;
    if (item) requestRemove(item);
  }
  function onScroll() { contextMenu = null; if (isActive()) scrollTop = viewport?.scrollTop ?? 0; }
  function filterTag(name: string) { selectedTags = selectedTags.includes(name) ? selectedTags.filter(t => t !== name) : [...selectedTags, name]; untagged = false; }
  function clearFilter() { selectedTags = []; untagged = false; materialBrowser.search = ""; }
  function beginImport(paths: string[]) {
    if (!isActive() || editorOpen || taskState.busy || anyModalOpen()) return;
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
  function requestRemove(item: Material | null = selected) {
    if (!item || deleting) return;
    contextMenu = null;
    deleteError = null;
    pendingDelete = { item, directory: libraryState.snapshot?.dataDirectory };
  }
  function cancelRemove() {
    if (deleting) return;
    pendingDelete = null; deleteError = null;
  }
  async function confirmRemove() {
    const target = pendingDelete;
    if (!target || deleting) return;
    const { item, directory } = target;
    if (directory !== libraryState.snapshot?.dataDirectory) { cancelRemove(); return; }
    deleting = true;
    deleteError = null;
    try {
      await deleteMaterial(item.id);
      if (directory !== libraryState.snapshot?.dataDirectory || pendingDelete !== target) return;
      pendingDelete = null;
      if (selected?.id === item.id) selected = null;
      thumbnails.clear(); covers.clear(); versionCovers.clear(); revision++;
      setNotice({ tone: "success", text: "素材已删除。" });
    }
    catch (cause) { if (pendingDelete === target) deleteError = `删除失败：${errorText(cause)}`; }
    finally { deleting = false; }
  }
  onMount(() => {
    const handler = (event: Event) => beginImport((event as CustomEvent<string[]>).detail);
    window.addEventListener("material-path-drop", handler);
    return () => window.removeEventListener("material-path-drop", handler);
  });
  onDestroy(() => { request++; pages.dispose(); thumbnails.dispose(); covers.dispose(); versionCovers.dispose(); });
  return {
    get thumbnails() { return thumbnails; },
    get versionCovers() { return versionCovers; },
    get covers() { return covers; },
    get pages() { return pages; },
    get total() { return total; },
    get loading() { return loading; },
    get error() { return error; },
    get tagError() { return tagError; },
    set tagError(value: typeof tagError) { tagError = value; },
    get selected() { return selected; },
    set selected(value: typeof selected) { selected = value; },
    get contextMenu() { return contextMenu; },
    set contextMenu(value: typeof contextMenu) { contextMenu = value; },
    get deleting() { return deleting; },
    set deleting(value: typeof deleting) { deleting = value; },
    get pendingDelete() { return pendingDelete; },
    set pendingDelete(value: typeof pendingDelete) { pendingDelete = value; },
    get deleteError() { return deleteError; },
    set deleteError(value: typeof deleteError) { deleteError = value; },
    get detailOpen() { return detailOpen; },
    set detailOpen(value: typeof detailOpen) { detailOpen = value; },
    get selectedTags() { return selectedTags; },
    set selectedTags(value: typeof selectedTags) { selectedTags = value; },
    get untagged() { return untagged; },
    set untagged(value: typeof untagged) { untagged = value; },
    get revision() { return revision; },
    set revision(value: typeof revision) { revision = value; },
    get editorOpen() { return editorOpen; },
    set editorOpen(value: typeof editorOpen) { editorOpen = value; },
    get editing() { return editing; },
    set editing(value: typeof editing) { editing = value; },
    get editingVersion() { return editingVersion; },
    set editingVersion(value: typeof editingVersion) { editingVersion = value; },
    get pendingPaths() { return pendingPaths; },
    set pendingPaths(value: typeof pendingPaths) { pendingPaths = value; },
    get editorKey() { return editorKey; },
    set editorKey(value: typeof editorKey) { editorKey = value; },
    get viewport() { return viewport; },
    set viewport(value: typeof viewport) { viewport = value; },
    get measuredWidth() { return measuredWidth; },
    set measuredWidth(value: typeof measuredWidth) { measuredWidth = value; },
    get measuredHeight() { return measuredHeight; },
    set measuredHeight(value: typeof measuredHeight) { measuredHeight = value; },
    get layout() { return layout; },
    get cells() { return cells; },
    get entries() { return entries; },
    showContextMenu,
    deleteFromMenu,
    onScroll,
    filterTag,
    clearFilter,
    chooseImages,
    create,
    editItem,
    edit,
    closeEditor,
    saved,
    copy,
    requestRemove,
    cancelRemove,
    confirmRemove,
  };
}
