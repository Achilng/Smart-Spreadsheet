<script lang="ts">
  import { onDestroy } from "svelte";
  import type { Material } from "../../api/materials";
  import { ImageLoader, isImageLoadCancelled } from "../../images/image-loader";
  import { errorText, setNotice } from "../../stores/app-state.svelte";
  import DetailPanelLayout from "../../ui/DetailPanelLayout.svelte";
  import DetailPreview from "../../ui/DetailPreview.svelte";
  import MaterialVersionTabs from "./MaterialVersionTabs.svelte";
  import { softFade } from "../../ui/motion";
  import DetailLightbox from "../../ui/DetailLightbox.svelte";

  let { material, loader, versionLoader, revision, active, onedit, ondelete, oncollapse }: {
    material: Material | null; loader: ImageLoader; versionLoader: ImageLoader; revision: number; active: boolean;
    onedit: (versionId?: number) => void; ondelete: () => void; oncollapse: () => void;
  } = $props();
  let selectedId = $state<number | null>(null);
  const version = $derived(material?.versions.find(v => v.id === selectedId) ?? material?.versions[0]);
  const materialId = $derived(material?.id);
  $effect(() => { void materialId; selectedId = null; });
  let src = $state<string | null>(null);
  let previewError = $state<string | null>(null);
  let lightboxOpen = $state(false);
  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | undefined;
  let selectionVersion = 0;
  let previewMaterialId: number | undefined;

  $effect(() => {
    void revision;
    const current = material;
    const currentVersion = version;
    const imageLoader = currentVersion?.hasImage ? versionLoader : loader;
    const id = currentVersion?.hasImage ? currentVersion.id : current?.id;
    selectionVersion++;
    copied = false;
    clearTimeout(copyTimer);
    lightboxOpen = false;
    previewError = null;
    const cached = id === undefined ? null : imageLoader.cached(id);
    if (cached || id === undefined || previewMaterialId !== current?.id) src = cached;
    previewMaterialId = current?.id;
    if (id === undefined) return;
    let cancelled = false;
    void imageLoader.load(id).then(async url => {
      const image = new Image(); image.src = url;
      await image.decode().catch(() => {});
      if (!cancelled) src = url;
    }).catch(error => {
      if (!cancelled && !isImageLoadCancelled(error)) { previewError = errorText(error); src = null; }
    });
    return () => { cancelled = true; };
  });
  $effect(() => { if (!active) lightboxOpen = false; });

  async function copyText() {
    if (!version?.text) return;
    const token = selectionVersion;
    try {
      await navigator.clipboard.writeText(version.text);
      if (selectionVersion !== token) return;
      copied = true;
      clearTimeout(copyTimer);
      copyTimer = setTimeout(() => copied = false, 1200);
    } catch { setNotice({ tone: "error", text: "复制失败，请检查剪贴板权限。" }); }
  }
  onDestroy(() => { selectionVersion++; clearTimeout(copyTimer); });
</script>

<DetailPanelLayout title={material?.title ?? "详情"} empty={!material} emptyText="点击素材卡片查看详情" onedit={() => onedit(version?.id)} {ondelete} {oncollapse}>
  {#if material}
    <DetailPreview {src} animate alt={`${material.title} · ${version?.name ?? ""}`} error={previewError} onopen={() => lightboxOpen = true} />
    {#if material.versions.length > 1}<MaterialVersionTabs versions={material.versions} selected={version?.id ?? 0} onselect={id => selectedId = id} />{/if}
    <section class="field">
      <div class="field-head"><h4>Tags</h4><button type="button" class="copy-btn" onclick={() => onedit(version?.id)}>编辑</button></div>
      <div class="chip-list">
        {#each material.tags as tag (tag)}<span class="chip">{tag}</span>{:else}<span class="faint">尚无 Tag</span>{/each}
      </div>
    </section>
    <section class="field">
      <div class="field-head">
        <h4>{version?.name ?? "文本内容"}</h4>
        <div class="field-head-actions">
          <button type="button" class="copy-btn" onclick={() => onedit(version?.id)}>编辑</button>
          {#if version?.text}<button type="button" class="copy-btn" onclick={() => void copyText()}>{copied ? "已复制" : "复制"}</button>{/if}
        </div>
      </div>
      <div class="version-text">{#key version?.id}<pre class:is-empty={!version?.text} in:softFade={{ duration: 140 }}>{version?.text || "—"}</pre>{/key}</div>
    </section>
  {/if}
</DetailPanelLayout>
<DetailLightbox bind:open={lightboxOpen} {src} alt={material?.title ?? "素材展示图"} />

<style>
  .version-text { min-height: 100px; }
  .version-text pre { height: 180px; box-sizing: border-box; }
</style>
