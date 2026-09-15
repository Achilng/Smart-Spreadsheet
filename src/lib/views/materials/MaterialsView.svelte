<script lang="ts">
  import { onDestroy, onMount, untrack } from "svelte";
  import { confirm, open } from "@tauri-apps/plugin-dialog";
  import { listMaterials, materialTagCounts, deleteMaterial, materialImage, type Material } from "../../api/materials";
  import type { TagSummary } from "../../api/tags";
  import { app, errorText, setNotice } from "../../stores/app-state.svelte";
  import { loadTags, tagStore } from "../../stores/tag-store.svelte";
  import { ImageLoader } from "../../images/image-loader";
  import { tagColorFor } from "../../utils/tag-colors";
  import { MATERIAL_IMAGE_EXTENSIONS, isMaterialImagePath } from "../../utils/materials";
  import { anyModalOpen } from "../../stores/modal-layer.svelte";
  import MaterialImage from "./MaterialImage.svelte";
  import MaterialEditor from "./MaterialEditor.svelte";

  let { active }: { active: boolean } = $props();
  const thumbnails = new ImageLoader(id => materialImage(id, true), 4, 144, "image/png");
  const covers = new ImageLoader(id => materialImage(id, false), 2, 8, "image/png");
  let items = $state<Material[]>([]);
  let tags = $state<TagSummary[]>([]);
  let selected = $state<Material | null>(null);
  let search = $state("");
  let tagSearch = $state("");
  let selectedTags = $state<string[]>([]);
  let untagged = $state(false);
  let offset = $state(0);
  let total = $state(0);
  let loading = $state(false);
  let error = $state("");
  let revision = $state(0);
  let editorOpen = $state(false);
  let editing = $state<Material | null>(null);
  let pendingPaths = $state<string[]>([]);
  let editorKey = $state(0);
  let request = 0;
  let lastDirectory: string | null | undefined;

  $effect(() => {
    const directory = app.snapshot?.dataDirectory;
    if (directory !== lastDirectory) {
      lastDirectory = directory;
      untrack(() => {
        request++; items = []; selected = null; search = ""; selectedTags = []; offset = 0;
        editorOpen = false; pendingPaths = []; thumbnails.clear(); covers.clear(); revision++;
      });
    }
  });
  $effect(() => {
    if (!active) return;
    void app.dataVersion; void tagStore.list; void revision;
    const query = search, filter = [...selectedTags], noTags = untagged, start = offset;
    // Invalidate the old response immediately, including during debounce.
    const token = ++request;
    const timer = setTimeout(() => void reload(token, query, filter, noTags, start), 180);
    return () => { clearTimeout(timer); request++; };
  });
  async function reload(token: number, query: string, filter: string[], noTags: boolean, start: number) {
    loading = true; error = "";
    try {
      const [page, counts] = await Promise.all([listMaterials(query, filter, noTags, start), materialTagCounts()]);
      if (token !== request) return;
      if (start > 0 && !page.items.length) { offset = Math.max(0, Math.floor((page.total - 1) / 48) * 48); return; }
      items = page.items; total = page.total; tags = counts;
      selected = selected ? items.find(item => item.id === selected?.id) ?? null : null;
    } catch (cause) { if (token === request) error = errorText(cause); }
    finally { if (token === request) loading = false; }
  }
  function filterTag(name: string) { selectedTags = selectedTags.includes(name) ? selectedTags.filter(t => t !== name) : [...selectedTags, name]; untagged = false; offset = 0; }
  function clearFilter() { selectedTags = []; untagged = false; search = ""; offset = 0; }
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
  function create() { editing = null; pendingPaths = []; editorKey++; editorOpen = true; }
  function edit() { if (selected) { editing = selected; pendingPaths = []; editorKey++; editorOpen = true; } }
  function closeEditor() { editorOpen = false; editing = null; pendingPaths = []; }
  function saved(item: Material) {
    thumbnails.clear(); covers.clear(); revision++;
    selected = item; clearFilter();
    void loadTags();
    setNotice({ tone: "success", text: `素材「${item.title}」已保存。` });
    if (!editing && pendingPaths.length > 1) { pendingPaths = pendingPaths.slice(1); editorKey++; }
    else closeEditor();
  }
  async function copy(item: Material) {
    if (!item.text) { setNotice({ tone: "error", text: "这份素材还没有文本，请先编辑内容。" }); return; }
    try { await navigator.clipboard.writeText(item.text); setNotice({ tone: "success", text: `已复制「${item.title}」的文本。` }); }
    catch (cause) { setNotice({ tone: "error", text: `复制失败：${errorText(cause)}` }); }
  }
  async function remove() {
    const item = selected;
    if (!item || !(await confirm(`删除素材「${item.title}」及其展示图和文本？原始图片不会被修改。`, { title: "删除素材", kind: "warning", okLabel: "删除", cancelLabel: "取消" }))) return;
    try { await deleteMaterial(item.id); selected = null; thumbnails.clear(); covers.clear(); revision++; setNotice({ tone: "success", text: "素材已删除。" }); }
    catch (cause) { setNotice({ tone: "error", text: errorText(cause) }); }
  }
  onMount(() => {
    const handler = (event: Event) => beginImport((event as CustomEvent<string[]>).detail);
    window.addEventListener("material-path-drop", handler);
    return () => window.removeEventListener("material-path-drop", handler);
  });
  onDestroy(() => { request++; thumbnails.dispose(); covers.dispose(); });
</script>

<section class="materials">
  <aside class="tag-sidebar">
    <h3>素材 Tag</h3>
    <input aria-label="搜索素材 Tag" bind:value={tagSearch} placeholder="搜索 Tag…" />
    <button class:current={!selectedTags.length && !untagged} onclick={clearFilter}>全部素材</button>
    <button class:current={untagged} onclick={() => { untagged = !untagged; selectedTags = []; offset = 0; }}>无 Tag 素材</button>
    <div class="tag-list">{#each tags.filter(t => t.name.toLowerCase().includes(tagSearch.toLowerCase())) as tag (tag.name)}
      {@const color = tagColorFor(tag.name, tagStore.list)}
      <button class:current={selectedTags.includes(tag.name)} aria-pressed={selectedTags.includes(tag.name)} onclick={() => filterTag(tag.name)}><i style:background={color.background}></i><span>{tag.name}</span><small>{tag.rowCount}</small></button>
    {/each}</div>
    <p>可多选 Tag，显示同时包含所选标签的素材。创建和编辑素材时可添加新 Tag。</p>
  </aside>
  <main>
    <header><div><h1>素材 <small>{total}</small></h1><p>把常用提示词存成图片卡片，双击即可复制。</p></div><div class="actions"><button class="btn" onclick={() => void chooseImages()}>导入图片</button><button class="btn btn-primary" onclick={create}>新建素材</button></div></header>
    <div class="search-line"><input aria-label="搜索素材名称和文本" value={search} placeholder="搜索素材名称或文本内容…" oninput={e => { search = e.currentTarget.value; offset = 0; }} />{#if selectedTags.length || untagged}<button class="btn" onclick={clearFilter}>清除筛选</button>{/if}</div>
    {#if selectedTags.length}<p class="filter-summary">Tag：{selectedTags.join("、")}</p>{/if}
    {#if error}<div class="empty" role="alert">{error}<button class="btn" onclick={() => revision++}>重试</button></div>
    {:else if !items.length}<div class="empty"><h2>{loading ? "正在读取素材…" : search || selectedTags.length || untagged ? "没有匹配的素材" : "收藏你的第一份素材"}</h2><p>新建素材可从图库选图，也可以将本地图片拖到这里导入。</p></div>
    {:else}<div class="grid" aria-busy={loading}>
      {#each items as item (`${item.id}-${revision}`)}
        <button class="card" class:is-selected={selected?.id === item.id} aria-pressed={selected?.id === item.id} title="单击查看详情，双击复制文本" onclick={() => selected = item} ondblclick={() => void copy(item)}>
          <div class="card-cover"><MaterialImage id={item.id} loader={thumbnails} alt={item.title} /></div>
          <strong>{item.title}</strong><div class="card-tags">{#each item.tags.slice(0, 3) as tag}{@const color = tagColorFor(tag, tagStore.list)}<span style:background={color.background} style:color={color.text}>{tag}</span>{/each}{#if item.tags.length > 3}<small>+{item.tags.length - 3}</small>{/if}</div>
        </button>
      {/each}
    </div>{/if}
    <footer><span>{loading ? "读取中…" : `${total} 份素材`}</span><div class="actions"><button class="btn" disabled={offset === 0 || loading} onclick={() => offset = Math.max(0, offset - 48)}>上一页</button><span>{Math.floor(offset / 48) + 1} / {Math.max(1, Math.ceil(total / 48))}</span><button class="btn" disabled={offset + 48 >= total || loading} onclick={() => offset += 48}>下一页</button></div></footer>
  </main>
  <aside class="detail">
    {#if selected}
      {#key `${selected.id}-${revision}`}<div class="detail-cover"><MaterialImage id={selected.id} loader={covers} alt={selected.title} /></div>{/key}
      <h2>{selected.title}</h2><div class="card-tags">{#each selected.tags as tag}{@const color = tagColorFor(tag, tagStore.list)}<span style:background={color.background} style:color={color.text}>{tag}</span>{/each}</div>
      <div class="actions"><button class="btn btn-primary" onclick={() => selected && void copy(selected)}>复制文本</button><button class="btn" onclick={edit}>编辑</button><button class="btn btn-danger" onclick={() => void remove()}>删除</button></div>
      <h3>文本内容</h3><pre>{selected.text || "尚未填写文本"}</pre>
    {:else}<div class="detail-empty"><h3>素材详情</h3><p>单击卡片查看和编辑<br />双击卡片复制文本</p></div>{/if}
  </aside>
</section>
{#if editorOpen}{#key editorKey}<MaterialEditor material={editing} path={pendingPaths[0] ?? null} remaining={Math.max(0, pendingPaths.length - 1)} loader={covers} onsaved={saved} onclose={closeEditor} />{/key}{/if}

<style>
  .materials { width: 100%; height: 100%; display: grid; grid-template-columns: 190px minmax(0,1fr) 330px; min-height: 0; color: var(--text); }
  .tag-sidebar { padding: 22px 12px; display: flex; flex-direction: column; gap: 10px; border-right: 1px solid var(--border); min-height: 0; }
  h1,h2,h3,p { margin: 0; } h1 { font-size: 25px; } h1 small { font-size: 14px; color: var(--text-3); font-weight: 400; } h3 { font-size: var(--font-sm); }
  input { width: 100%; min-width: 0; padding: 9px 10px; border: 1px solid var(--border); background: var(--surface); border-radius: 8px; color: var(--text); box-sizing: border-box; }
  .tag-sidebar button { display: flex; align-items: center; gap: 8px; padding: 9px; background: transparent; border: 0; border-radius: 7px; color: var(--text-2); text-align: left; }
  .tag-sidebar button.current { background: var(--accent-soft); color: var(--accent); } .tag-sidebar button:hover { background: var(--surface-2); }
  .tag-list { overflow-y: auto; min-height: 0; } .tag-list button { width: 100%; } .tag-list i { width: 9px; height: 9px; border-radius: 3px; flex: none; } .tag-list span { flex: 1; overflow: hidden; text-overflow: ellipsis; }
  .tag-sidebar p { margin-top: auto; font-size: 11px; color: var(--text-3); line-height: 1.7; }
  main { display: flex; flex-direction: column; min-height: 0; padding: 24px; gap: 17px; overflow: hidden; }
  header { display: flex; justify-content: space-between; gap: 14px; align-items: center; } header p { margin-top: 8px; color: var(--text-3); font-size: var(--font-sm); }
  .actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; } .search-line { display: flex; gap: 8px; } .filter-summary { color: var(--accent); font-size: var(--font-sm); }
  .grid { flex: 1; overflow-y: auto; display: grid; grid-template-columns: repeat(auto-fill, minmax(165px,1fr)); grid-auto-rows: max-content; gap: 17px; align-content: start; padding: 3px; min-height: 0; }
  .card { min-width: 0; min-height: 270px; padding: 0 0 12px; text-align: left; border: 1px solid var(--border); border-radius: 11px; background: var(--surface); overflow: hidden; color: var(--text); transition: border-color .15s, box-shadow .15s; }
  .card:hover { border-color: var(--accent); } .card.is-selected { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-soft); }
  .card-cover { height: 200px; background: var(--surface-2); } .card strong { display: block; padding: 11px 12px 8px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--font-sm); }
  .card-tags { display: flex; flex-wrap: wrap; gap: 5px; } .card .card-tags { padding: 0 12px; min-height: 20px; } .card-tags span { border-radius: 5px; padding: 3px 6px; font-size: 11px; max-width: 100%; overflow-wrap: anywhere; }
  .empty { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 15px; color: var(--text-3); text-align: center; } .empty h2 { font-size: 20px; color: var(--text-2); }
  footer { display: flex; align-items: center; justify-content: space-between; color: var(--text-3); font-size: var(--font-sm); }
  .detail { padding: 22px 18px; overflow-y: auto; display: flex; flex-direction: column; gap: 17px; border-left: 1px solid var(--border); background: var(--surface); }
  .detail-cover { height: 270px; min-height: 180px; flex: none; border-radius: 9px; overflow: hidden; background: var(--surface-2); } .detail h2 { font-size: 19px; overflow-wrap: anywhere; }
  pre { white-space: pre-wrap; overflow-wrap: anywhere; font-family: inherit; font-size: var(--font-sm); line-height: 1.75; margin: 0; user-select: text; } .detail-empty { color: var(--text-3); text-align: center; margin-top: 80px; line-height: 2; }
  @media (max-width: 1150px) { .materials { grid-template-columns: 155px minmax(0,1fr) 275px; } main { padding: 18px; } header { align-items: start; flex-direction: column; } }
  @media (max-width: 850px) { .materials { grid-template-columns: minmax(0,1fr) 255px; } .tag-sidebar { display: none; } }
</style>
