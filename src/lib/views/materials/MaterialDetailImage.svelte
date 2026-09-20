<script lang="ts">
  import { ImageLoader, isImageLoadCancelled } from "../../images/image-loader";
  import { errorText } from "../../stores/app-state.svelte";
  import DetailPreview from "../../ui/DetailPreview.svelte";
  import DetailLightbox from "../../ui/DetailLightbox.svelte";

  let { id, loader, revision, active, alt }: { id: number; loader: ImageLoader; revision: number; active: boolean; alt: string } = $props();
  let container = $state<HTMLDivElement | null>(null);
  let visible = $state(false), lightboxOpen = $state(false);
  let src = $state<string | null>(null), error = $state<string | null>(null);
  $effect(() => {
    if (!container) return;
    const observer = new IntersectionObserver(entries => visible = entries[0].isIntersecting,
      { root: container.closest(".panel-scroll"), rootMargin: "120px 0px" });
    observer.observe(container);
    return () => observer.disconnect();
  });
  $effect(() => {
    void revision;
    if (!active || (!visible && !lightboxOpen)) { src = null; return; }
    let cancelled = false;
    error = null;
    src = loader.cached(id);
    void loader.load(id).then(url => { if (!cancelled) src = url; }).catch(cause => {
      if (!cancelled && !isImageLoadCancelled(cause)) error = errorText(cause);
    });
    return () => { cancelled = true; };
  });
  $effect(() => { void id; void revision; lightboxOpen = false; });
  $effect(() => { if (!active) lightboxOpen = false; });
</script>

<div class="material-detail-image" bind:this={container}>
  <DetailPreview {src} {alt} {error} onopen={() => lightboxOpen = true} />
</div>
<DetailLightbox bind:open={lightboxOpen} {src} {alt} />

<style>
  .material-detail-image { flex: none; min-width: 0; }
</style>
