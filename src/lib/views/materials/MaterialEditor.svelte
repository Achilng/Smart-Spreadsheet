<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { confirm, open } from "@tauri-apps/plugin-dialog";
  import { inspectMaterialImage, inspectMaterialLibraryImage, saveMaterial, type Material, type MaterialDraft, type MaterialInspection } from "../../api/materials";
  import { app, errorText } from "../../stores/app-state.svelte";
  import { tagStore } from "../../stores/tag-store.svelte";
  import { tagColorFor } from "../../utils/tag-colors";
  import { MATERIAL_IMAGE_EXTENSIONS, mergeMaterialMetadata, splitMaterialTags } from "../../utils/materials";
  import { registerCloseGuard } from "../../stores/close-guard";
  import Modal from "../../ui/Modal.svelte";
  import MaterialImage from "./MaterialImage.svelte";
  import { startMaterialGalleryPick } from "../../stores/material-gallery-picker.svelte";
  import type { ImageLoader } from "../../images/image-loader";

  let { material = null, path = null, remaining = 0, loader, onsaved, onclose }: {
    material?: Material | null; path?: string | null; remaining?: number; loader: ImageLoader;
    onsaved: (item: Material) => void; onclose: () => void;
  } = $props();
  let draft = $state<MaterialDraft>({ id: null, title: "", text: "", tags: [], imagePath: null });
  let initial = $state("");
  let inspection = $state<MaterialInspection | null>(null);
  let preview = $state("");
  let metadataMode = $state(false);
  let selectedSections = $state<string[]>([]);
  let busy = $state(false);
  let error = $state("");
  let tagQuery = $state("");
  let libraryOpen = $state(false);
  let stopGalleryPick: (() => void) | undefined;
  const availableTags = $derived([...new Set([...tagStore.list.map(t => t.name), ...draft.tags])].filter(name => name.toLowerCase().includes(tagQuery.trim().toLowerCase())));
  const combined = $derived(mergeMaterialMetadata(inspection?.sections ?? [], selectedSections));
  const dirty = $derived(JSON.stringify(draft) !== initial);

  onMount(() => {
    draft = { id: material?.id ?? null, title: material?.title ?? "", text: material?.text ?? "", tags: [...(material?.tags ?? [])], imagePath: null };
    initial = JSON.stringify(draft);
    if (path) void inspectImage(path);
    return registerCloseGuard(() => busy ? "素材正在读取或保存" : dirty ? "素材有尚未确认保存的内容" : null);
  });
  onDestroy(() => { stopGalleryPick?.(); if (preview) URL.revokeObjectURL(preview); });

  function chooseFromGallery() {
    error = ""; libraryOpen = true;
    stopGalleryPick = startMaterialGalleryPick(rowId => {
      libraryOpen = false;
      if (rowId !== null) void chooseLibraryImage(rowId);
    });
  }

  async function inspectImage(imagePath: string) {
    busy = true; error = "";
    try {
      const result = await inspectMaterialImage(imagePath);
      applyImage(imagePath, result);
    } catch (cause) { error = `无法读取图片：${errorText(cause)}`; }
    finally { busy = false; }
  }
  function applyImage(imagePath: string, result: MaterialInspection) {
    if (preview) URL.revokeObjectURL(preview);
    preview = URL.createObjectURL(new Blob([new Uint8Array(result.preview)], { type: "image/png" }));
    inspection = result;
    draft.imagePath = imagePath;
    if (!draft.title.trim()) draft.title = result.title;
    metadataMode = false; selectedSections = [];
  }
  async function chooseLibraryImage(rowId: number) {
    busy = true; error = "";
    try {
      const result = await inspectMaterialLibraryImage(rowId);
      applyImage(result.path, result.inspection);
      libraryOpen = false;
    } catch (cause) { error = `无法读取图片：${errorText(cause)}`; }
    finally { busy = false; }
  }
  async function chooseImage() {
    try {
      const result = await open({ multiple: false, directory: false, title: "选择素材展示图", filters: [{ name: "图片", extensions: MATERIAL_IMAGE_EXTENSIONS }] });
      if (typeof result === "string") await inspectImage(result);
    } catch (cause) { error = errorText(cause); }
  }
  async function close() {
    if (busy) return;
    if (libraryOpen) { libraryOpen = false; error = ""; return; }
    if (dirty && !(await confirm("放弃尚未保存的素材内容？", { title: "取消编辑", kind: "warning", okLabel: "放弃", cancelLabel: "继续编辑" }))) return;
    onclose();
  }
  function toggleTag(name: string) { draft.tags = draft.tags.includes(name) ? draft.tags.filter(t => t !== name) : [...draft.tags, name]; }
  function addTags() { draft.tags = [...new Set([...draft.tags, ...splitMaterialTags(tagQuery)])]; tagQuery = ""; }
  async function applyMetadata() {
    if (draft.text && draft.text !== combined && !(await confirm("用所选元数据替换当前文本内容？", { title: "填入元数据", okLabel: "替换", cancelLabel: "取消" }))) return;
    draft.text = combined;
  }
  async function save() {
    if (busy || (!draft.id && !draft.imagePath)) return;
    busy = true; error = "";
    try { const item = await saveMaterial($state.snapshot(draft)); initial = JSON.stringify(draft); onsaved(item); }
    catch (cause) { error = `保存失败，内容已保留：${errorText(cause)}`; }
    finally { busy = false; }
  }
</script>

<Modal open={!libraryOpen && app.viewMode === "materials"} onclose={() => void close()} {busy} width="900px" labelledby="material-editor-title">
  <div class="editor">
    <header><h2 id="material-editor-title">{material ? "编辑素材" : "确认导入素材"}</h2><span>{remaining > 0 ? `之后还有 ${remaining} 张待确认` : material ? "保存后修改才会生效" : "确认保存后才会添加到素材库"}</span></header>
    <div class="editor-body">
      <div class="cover-column">
        <div class="cover">{#if preview}<img src={preview} alt="待保存的素材展示图" />{:else if material}<MaterialImage id={material.id} {loader} alt={material.title} />{:else}<span>选择一张展示图</span>{/if}</div>
        <button class="btn btn-primary" disabled={busy} onclick={chooseFromGallery}>去画廊选择</button>
        <button class="btn" disabled={busy} onclick={() => void chooseImage()}>{busy ? "处理中…" : "从本地文件选择"}</button>
        <p>支持 PNG、JPG、WebP 等图片。展示图随素材保存。</p>
      </div>
      <fieldset disabled={busy}>
        <label>名称<input bind:value={draft.title} placeholder="例如：黑金礼服" /></label>
        <div class="tag-section">
          <label>Tag<input bind:value={tagQuery} placeholder="搜索或新建 Tag，多个用逗号分隔" onkeydown={e => { if (e.key === "Enter" && !e.isComposing) { e.preventDefault(); addTags(); } }} /></label>
          {#if tagQuery.trim()}<button class="btn" onclick={addTags}>添加输入的 Tag</button>{/if}
          <div class="tags">
            {#each availableTags as name (name)}{@const color = tagColorFor(name, tagStore.list)}
              <button class:selected={draft.tags.includes(name)} aria-pressed={draft.tags.includes(name)} style:--tag-color={color.background} onclick={() => toggleTag(name)}>{draft.tags.includes(name) ? "✓ " : "+ "}{name}</button>
            {/each}
          </div>
          <small>{draft.tags.length ? `已选：${draft.tags.join("、")}` : "未选择 Tag"}</small>
        </div>
        {#if inspection}
          {#if inspection.warning}<p class="hint">{inspection.warning}</p>{/if}
          {#if inspection.sections.length}
            <div class="metadata">
              <strong>检测到元数据，要用作文本内容吗？</strong>
              <label class="inline"><input type="checkbox" bind:checked={metadataMode} />从元数据选择内容</label>
              {#if metadataMode}
                <div class="metadata-options">{#each inspection.sections as section (section.id)}<label class="inline"><input type="checkbox" bind:group={selectedSections} value={section.id} />{section.label}</label>{/each}</div>
                <label>所选内容预览<textarea class="metadata-preview" readonly value={combined} placeholder="请选择一个或多个区域"></textarea></label>
                <button class="btn" disabled={!selectedSections.length} onclick={() => void applyMetadata()}>将所选内容填入下方文本</button>
              {/if}
            </div>
          {:else}<p class="hint">未发现可提取的提示词文本，可在下方手动填写。</p>{/if}
        {/if}
        <label>文本内容<textarea class="content" bind:value={draft.text} placeholder="粘贴提示词或其他文本；双击素材卡片会复制这里的全部内容。"></textarea></label>
      </fieldset>
    </div>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <footer><button class="btn" disabled={busy} onclick={() => void close()}>{remaining ? "取消剩余导入" : "取消"}</button><button class="btn btn-primary" disabled={busy || (!draft.id && !draft.imagePath)} onclick={() => void save()}>{busy ? "处理中…" : material ? "保存修改" : remaining ? "确认导入，继续下一张" : "确认导入"}</button></footer>
  </div>
</Modal>

<style>
  .editor { padding: 22px; display: grid; gap: 18px; max-height: 85vh; overflow-y: auto; }
  header, footer { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  h2 { font-size: var(--font-xl); margin: 0; } header span, p, small { font-size: var(--font-sm); color: var(--text-3); }
  .editor-body { display: grid; grid-template-columns: 250px minmax(0,1fr); gap: 22px; }
  .cover-column { display: flex; flex-direction: column; gap: 12px; }
  .cover { height: 310px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface-2); display: grid; place-items: center; overflow: hidden; color: var(--text-3); }
  .cover img { width: 100%; height: 100%; object-fit: contain; }
  fieldset { display: flex; flex-direction: column; gap: 16px; min-width: 0; margin: 0; padding: 0; border: 0; }
  label { display: flex; flex-direction: column; gap: 7px; font-size: var(--font-sm); }
  input:not([type=checkbox]), textarea { width: 100%; padding: 9px 11px; border: 1px solid var(--border); border-radius: 7px; background: var(--surface); color: var(--text); font: inherit; box-sizing: border-box; }
  textarea { resize: vertical; line-height: 1.6; } .content { min-height: 180px; } .metadata-preview { height: 100px; }
  .tag-section, .metadata { display: grid; gap: 9px; } .metadata { background: var(--surface-2); padding: 12px; border-radius: 8px; }
  .tags { display: flex; flex-wrap: wrap; gap: 6px; max-height: 110px; overflow-y: auto; }
  .tags button { border: 1px solid var(--border); border-radius: var(--radius-full); padding: 5px 9px; background: var(--surface); color: var(--text-2); }
  .tags button.selected { border-color: var(--tag-color); background: var(--accent-soft); color: var(--text); }
  .metadata-options { max-height: 140px; overflow-y: auto; display: grid; gap: 7px; }
  .inline { flex-direction: row; align-items: center; } .hint { margin: 0; line-height: 1.5; }
  .error { color: var(--danger); } footer { justify-content: flex-end; }
  @media (max-width: 720px) { .editor-body { grid-template-columns: 1fr; } .cover { height: 190px; } header { align-items: start; flex-direction: column; } }
</style>
