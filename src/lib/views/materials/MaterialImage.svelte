<script lang="ts">
  import { ImageLoader, isImageLoadCancelled } from "../../images/image-loader";
  let { id, loader, alt, fit = "contain" }: { id: number; loader: ImageLoader; alt: string; fit?: "contain" | "cover" } = $props();
  let src = $state("");
  let failed = $state(false);
  $effect(() => {
    let disposed = false;
    src = ""; failed = false;
    void loader.load(id).then(url => { if (!disposed) src = url; }).catch(error => {
      if (!disposed && !isImageLoadCancelled(error)) failed = true;
    });
    return () => { disposed = true; };
  });
</script>
{#if src}<img {src} {alt} style:object-fit={fit} />{:else}<span class="placeholder">{failed ? "展示图加载失败" : "加载图片…"}</span>{/if}
<style>
  img { display: block; width: 100%; height: 100%; object-fit: contain; }
  .placeholder { display: grid; place-content: center; width: 100%; height: 100%; color: var(--text-3); font-size: var(--font-sm); }
</style>
