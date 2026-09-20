<script lang="ts">
  import type { Material } from "../../api/materials";
  import type { ImageLoader } from "../../images/image-loader";
  import DetailPanelLayout from "../../ui/DetailPanelLayout.svelte";
  import MaterialDetailImage from "./MaterialDetailImage.svelte";
  import MaterialVersionDetail from "./MaterialVersionDetail.svelte";

  let { material, loader, versionLoader, revision, active, onedit, ondelete, oncollapse }: {
    material: Material | null; loader: ImageLoader; versionLoader: ImageLoader; revision: number; active: boolean;
    onedit: (versionId?: number) => void; ondelete: () => void; oncollapse: () => void;
  } = $props();
</script>

<DetailPanelLayout title={material?.title ?? "详情"} empty={!material} emptyText="点击素材卡片查看详情" onedit={() => onedit()} {ondelete} {oncollapse}>
  {#if material}
    <MaterialDetailImage id={material.id} {loader} {revision} {active} alt={material.title} />
    <section class="field shared-tags">
      <div class="field-head"><h4>Tags</h4><button type="button" class="copy-btn" onclick={() => onedit()}>编辑</button></div>
      <div class="chip-list">
        {#each material.tags as tag (tag)}<span class="chip">{tag}</span>{:else}<span class="faint">尚无 Tag</span>{/each}
      </div>
    </section>
    {#each material.versions as version, index (`${material.id}-${version.id}`)}
      <MaterialVersionDetail {version} isDefault={index === 0 && material.versions.length > 1} loader={versionLoader} {revision} {active} onedit={() => onedit(version.id)} />
    {/each}
  {/if}
</DetailPanelLayout>

<style>
  .shared-tags { flex: none; min-width: 0; }
  .shared-tags .field-head { gap: 12px; margin-bottom: 8px; }
  .shared-tags .field-head h4 { margin: 0; }
  .shared-tags .chip-list { margin-bottom: 0; }
</style>
