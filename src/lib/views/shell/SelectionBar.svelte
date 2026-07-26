<script lang="ts">
  import { app, formatCount } from "../../stores/app-state.svelte";
  import { deletion, requestDelete } from "../../stores/delete-actions.svelte";
  import { buildExportItems } from "../../stores/export-actions";
  import { anyModalOpen } from "../../stores/modal-layer.svelte";
  import {
    clearSelection,
    getSelectedCount,
    selectAllFiltered,
    selection,
    selectionDto,
    selectionIds,
  } from "../../stores/selection-store.svelte";
  import { rowStore } from "../../stores/row-store.svelte";
  import Dropdown from "../../ui/Dropdown.svelte";
  import GroupAssignDialog from "../groups/GroupAssignDialog.svelte";
  import PromptEditDialog from "./PromptEditDialog.svelte";
  import { softFade, softFly } from "../../ui/motion";

  let promptEditOpen = $state(false);
  let groupDialogOpen = $state(false);

  const count = $derived(getSelectedCount());
  const exportItems = $derived(buildExportItems());
  let selectingAll = $state(false);

  async function selectAll(): Promise<void> {
    if (selectingAll) {
      return;
    }
    selectingAll = true;
    try {
      await selectAllFiltered();
    } finally {
      selectingAll = false;
    }
  }

  function startTagging(): void {
    app.sidebarMode = "tag";
  }

  function dismiss(): void {
    clearSelection();
  }
</script>

<svelte:window
  onkeydown={event => {
    // 模态浮层打开时 Esc 归浮层处理，不清空选区（否则依赖选区的对话框会被强制卸载）。
    if (
      event.key === "Escape" &&
      count > 0 &&
      !deletion.open &&
      !anyModalOpen() &&
      !event.defaultPrevented
    ) {
      clearSelection();
    }
  }}
/>

{#if count > 0}
  <div class="selection-bar" transition:softFly={{ duration: 180, y: 14 }}>
    <span class="count tabular">
      已选
      {#key count}
        <span class="count-value" transition:softFade={{ duration: 110 }}>{formatCount(count)}</span>
      {/key}
      张
      {#if selection.kind === "filtered"}
        <small class="faint">（筛选全选{selectionIds.size > 0 ? `，排除 ${formatCount(selectionIds.size)} 张` : ""}）</small>
      {/if}
    </span>
    {#if selection.kind === "explicit" && count < rowStore.totalCount}
      <button
        type="button"
        class="btn sel-act"
        disabled={selectingAll}
        onclick={() => void selectAll()}
        transition:softFade={{ duration: 120 }}
      >
        {selectingAll ? "全选中…" : "全选"}
      </button>
    {/if}
    <button
      type="button"
      class="btn btn-primary sel-act"
      onclick={startTagging}
    >
      打 Tag
    </button>
    <button
      type="button"
      class="btn sel-act"
      onclick={() => (groupDialogOpen = true)}
    >
      移入分组
    </button>
    <button
      type="button"
      class="btn sel-act"
      onclick={() => (promptEditOpen = true)}
    >
      编辑提示词
    </button>
    <Dropdown label="导出所选" items={exportItems} direction="up" />
    <button
      type="button"
      class="btn btn-danger sel-act"
      onclick={() => requestDelete(selectionDto(), count)}
    >
      删除
    </button>
    <button type="button" class="quit" onclick={dismiss}>
      取消选择
    </button>
  </div>
{/if}

{#if groupDialogOpen && count > 0}
  <GroupAssignDialog
    selection={selectionDto()}
    {count}
    onclose={() => (groupDialogOpen = false)}
  />
{/if}

{#if promptEditOpen && count > 0}
  <PromptEditDialog
    selection={selectionDto()}
    {count}
    onclose={() => (promptEditOpen = false)}
  />
{/if}

<style>
  /* 底部通栏：白底 + 上发丝线（覆盖在画布之上，不挤压虚拟列表） */
  .selection-bar {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    min-height: 52px;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    padding: 8px 26px;
    background: var(--surface);
    border-top: 1px solid var(--border);
    z-index: var(--z-nav);
    white-space: nowrap;
  }

  @media (max-width: 1080px) {
    .selection-bar {
      padding: 8px 14px;
    }
  }

  .count {
    font-size: var(--font-md);
    font-weight: 700;
    margin-right: 10px;
    flex: none;
  }

  .count small {
    font-weight: 400;
    font-size: var(--font-sm);
  }

  .count-value {
    display: inline-block;
    min-width: 1ch;
    text-align: center;
  }

  .sel-act {
    padding: 5px 15px;
    font-size: var(--font-sm);
    min-height: 32px;
    flex: none;
  }

  .quit {
    margin-left: auto;
    border: none;
    background: none;
    font-size: 12.5px;
    color: var(--accent);
    padding: 4px 8px;
    flex: none;
  }

  .quit:hover {
    text-decoration: underline;
  }
</style>
