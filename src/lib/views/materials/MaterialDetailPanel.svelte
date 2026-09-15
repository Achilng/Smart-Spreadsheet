<script lang="ts">
  import { onDestroy } from "svelte";
  import type { Material } from "../../api/materials";
  import { ImageLoader, isImageLoadCancelled } from "../../images/image-loader";
  import { errorText, setNotice } from "../../stores/app-state.svelte";
  import DetailPanelLayout from "../../ui/DetailPanelLayout.svelte";
  import DetailPreview from "../../ui/DetailPreview.svelte";
  import DetailLightbox from "../../ui/DetailLightbox.svelte";

  let { material, loader, revision, active, onedit, ondelete, oncollapse }: {
    material: Material | null; loader: ImageLoader; revision: number; active: boolean;
    onedit: () => void; ondelete: () => void; oncollapse: () => void;
  } = $props();
  let src = $state<string | null>(null);
  let previewError = $state<string | null>(null);
  let lightboxOpen = $state(false);
  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | undefined;
  let selectionVersion = 0;

  $effect(() => {
    void revision;
    const current = material;
    const id = current?.id;
    selectionVersion++;
    copied = false;
    clearTimeout(copyTimer);
    lightboxOpen = false;
    previewError = null;
    src = id === undefined ? null : loader.cached(id);
    if (id === undefined) return;
    let cancelled = false;
    void loader.load(id).then(url => { if (!cancelled) src = url; }).catch(error => {
      if (!cancelled && !isImageLoadCancelled(error)) previewError = errorText(error);
    });
    return () => { cancelled = true; };
  });
  $effect(() => { if (!active) lightboxOpen = false; });

  async function copyText() {
    if (!material?.text) return;
    const version = selectionVersion;
    try {
      await navigator.clipboard.writeText(material.text);
      if (selectionVersion !== version) return;
      copied = true;
      clearTimeout(copyTimer);
      copyTimer = setTimeout(() => copied = false, 1200);
    } catch { setNotice({ tone: "error", text: "复制失败，请检查剪贴板权限。" }); }
  }
  onDestroy(() => { selectionVersion++; clearTimeout(copyTimer); });
</script>

<DetailPanelLayout title={material?.title ?? "详情"} empty={!material} emptyText="点击素材卡片查看详情" {onedit} {ondelete} {oncollapse}>
  {#if material}
    <DetailPreview {src} alt={material.title} error={previewError} onopen={() => lightboxOpen = true} />
    <section class="field">
      <div class="field-head"><h4>Tags</h4><button type="button" class="copy-btn" onclick={onedit}>编辑</button></div>
      <div class="chip-list">
        {#each material.tags as tag (tag)}<span class="chip">{tag}</span>{:else}<span class="faint">尚无 Tag</span>{/each}
      </div>
    </section>
    <section class="field">
      <div class="field-head">
        <h4>文本内容</h4>
        <div class="field-head-actions">
          <button type="button" class="copy-btn" onclick={onedit}>编辑</button>
          {#if material.text}<button type="button" class="copy-btn" onclick={() => void copyText()}>{copied ? "已复制" : "复制"}</button>{/if}
        </div>
      </div>
      <pre class:is-empty={!material.text}>{material.text || "—"}</pre>
    </section>
  {/if}
</DetailPanelLayout>
<DetailLightbox bind:open={lightboxOpen} {src} alt={material?.title ?? "素材展示图"} />
