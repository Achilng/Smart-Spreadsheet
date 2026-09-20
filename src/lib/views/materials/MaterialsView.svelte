<script lang="ts">
  import TagFilterSidebar from "../../ui/TagFilterSidebar.svelte";
  import { materialBrowser } from "../../stores/material-browser.svelte";
  import GalleryTile from "../gallery/GalleryTile.svelte";
  import GalleryViewport from "../gallery/GalleryViewport.svelte";
  import SizeSlider from "../shell/SizeSlider.svelte";
  import Thumbnail from "../../ui/Thumbnail.svelte";
  import MaterialDetailPanel from "./MaterialDetailPanel.svelte";
  import DetailSidebar from "../../ui/DetailSidebar.svelte";
  import MaterialVersionMenu from "./MaterialVersionMenu.svelte";
  import MaterialEditor from "./MaterialEditor.svelte";
  import ContextMenuShell from "../../ui/ContextMenuShell.svelte";
  import DeleteConfirmation from "../../ui/DeleteConfirmation.svelte";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import { createMaterialsController } from "../../features/materials/controller.svelte";

  let { active }: { active: boolean } = $props();
  const controller = createMaterialsController(() => active);
</script>

<svelte:window onresize={() => controller.contextMenu = null} />

<section class="materials" inert={controller.editorOpen || controller.pendingDelete !== null}>
  <aside class="filter-sidebar">
    <TagFilterSidebar entries={controller.entries} activeTags={controller.selectedTags} ontoggle={controller.filterTag}
      summary={controller.selectedTags.length || controller.untagged ? `${controller.selectedTags.length} 个 Tag · ${Number(controller.untagged)} 个条件生效` : "未启用筛选"}
      modeLabel="同时匹配" error={controller.tagError} emptyText="还没有 Tag。编辑素材时可以添加。">
      {#snippet filters()}
        <div class="f-group" role="group" aria-label="素材显示条件">
          <div class="f-head">显示</div>
          <label class="check-row" class:on={controller.untagged}>
            <input type="checkbox" checked={controller.untagged} onchange={() => { controller.untagged = !controller.untagged; controller.selectedTags = []; }} />
            <span class="cbox" aria-hidden="true"></span>无 Tag 素材
          </label>
          {#if controller.selectedTags.length || controller.untagged || materialBrowser.search}<button type="button" class="mode-link" onclick={controller.clearFilter}>清除全部筛选</button>{/if}
        </div>
      {/snippet}
    </TagFilterSidebar>
  </aside>
  <main>
    <header><div><h1>素材 <small>{controller.total}</small></h1></div><div class="actions"><SizeSlider /><button class="btn" onclick={() => void controller.chooseImages()}>导入图片</button><button class="btn btn-primary" onclick={controller.create}>新建素材</button></div></header>
    {#if controller.selectedTags.length}<p class="filter-summary">Tag：{controller.selectedTags.join("、")}</p>{/if}
    {#if controller.error}<div class="load-error" role="alert">{controller.error}<button class="btn" onclick={() => controller.pages.retry()}>重试</button></div>{/if}
    <GalleryViewport bind:viewport={controller.viewport} bind:measuredWidth={controller.measuredWidth} bind:measuredHeight={controller.measuredHeight} onscroll={controller.onScroll} spacerHeight={controller.total ? controller.layout.spacerHeight : undefined} busy={controller.loading}>
      {#if !controller.total}
        <div class="empty"><h2>{controller.loading ? "正在读取素材…" : controller.error ? "素材读取失败" : materialBrowser.search || controller.selectedTags.length || controller.untagged ? "没有匹配的素材" : "收藏你的第一份素材"}</h2>
          {#if !controller.loading && !controller.error}<p>新建素材可从图库选图，也可以将本地图片拖到这里导入。</p>{/if}
        </div>
      {:else}
        {#each controller.cells as cell (`${cell.index}-${controller.revision}`)}
          {@const item = cell.item}
          <GalleryTile x={cell.x} y={cell.y} width={controller.layout.cardWidth} imageHeight={controller.layout.imageHeight}
            skeleton={!item} title={item?.title ?? ""} titleAlign="center" tags={item?.tags ?? []} isActive={!!item && controller.selected?.id === item.id}
            onclick={() => { if (item) controller.selected = item; }} ondblclick={() => { if (item) void controller.copy(item); }}
            oncontextmenu={event => { if (item) controller.showContextMenu(event, item); }}>
            {#snippet overlayControls()}{#if item && item.versions.length > 1}<MaterialVersionMenu material={item} onmanage={() => controller.editItem(item)} />{/if}{/snippet}
            {#snippet image()}{#if item}<Thumbnail rowId={item.id} loader={controller.thumbnails} previewLoader={null} allowFileDrag={false} hasImage={true} alt={item.title} />{/if}{/snippet}
          </GalleryTile>
        {/each}
      {/if}
    </GalleryViewport>
  </main>
  <DetailSidebar open={controller.detailOpen} onopen={() => controller.detailOpen = true}>
    <MaterialDetailPanel material={controller.selected} revision={controller.revision} active={active && !controller.editorOpen && !controller.pendingDelete} loader={controller.covers} versionLoader={controller.versionCovers}
      onedit={controller.edit} ondelete={() => controller.requestRemove()} oncollapse={() => controller.detailOpen = false} />
  </DetailSidebar>
</section>
<ContextMenuShell open={controller.contextMenu !== null} x={controller.contextMenu?.x ?? 0} y={controller.contextMenu?.y ?? 0} onclose={() => controller.contextMenu = null}>
  <button type="button" role="menuitem" class="danger menu-delete" disabled={controller.deleting} onclick={controller.deleteFromMenu}><Trash2 size={14} strokeWidth={1.6} />删除素材</button>
</ContextMenuShell>
<DeleteConfirmation open={controller.pendingDelete !== null} title={`删除素材「${controller.pendingDelete?.item.title ?? ""}」？`}
  warning="删除后无法通过 Ctrl+Z 恢复。"
  description={`该素材的全部 ${controller.pendingDelete?.item.versions.length ?? 0} 个版本、文本及展示图将一并删除，原始图片不会被修改。`}
  busy={controller.deleting} error={controller.deleteError} oncancel={controller.cancelRemove} onconfirm={() => void controller.confirmRemove()} />
{#if controller.editorOpen}{#key controller.editorKey}<MaterialEditor material={controller.editing} versionId={controller.editingVersion} path={controller.pendingPaths[0] ?? null} remaining={Math.max(0, controller.pendingPaths.length - 1)} loader={controller.covers} versionLoader={controller.versionCovers} onsaved={controller.saved} onclose={controller.closeEditor} />{/key}{/if}

<style>
  .menu-delete { gap: 8px; }
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
