<script lang="ts">
  import { formatCount } from "../../stores/app-state.svelte";
  import { deletion, requestDelete } from "../../stores/delete-actions.svelte";
  import {
    clearSelection,
    getSelectedCount,
    selectAllFiltered,
    selection,
    selectionDto,
    selectionIds,
  } from "../../stores/selection-store.svelte";
  import { rowStore } from "../../stores/row-store.svelte";
  import GroupAssignDialog from "../groups/GroupAssignDialog.svelte";
  import PromptEditDialog from "./PromptEditDialog.svelte";

  let promptEditOpen = $state(false);
  let groupDialogOpen = $state(false);

  const count = $derived(getSelectedCount());
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

  function dismiss(): void {
    clearSelection();
  }
</script>

<svelte:window
  onkeydown={event => {
    if (event.key === "Escape" && count > 0 && !deletion.open) {
      clearSelection();
    }
  }}
/>

{#if count > 0}
  <div class="selection-bar-wrap">
    <div class="selection-bar">
      <span class="count">
        已选 {formatCount(count)} 行
        {#if selection.kind === "filtered"}
          <small class="faint">（筛选全选{selectionIds.size > 0 ? `，排除 ${formatCount(selectionIds.size)} 行` : ""}）</small>
        {/if}
      </span>
      {#if selection.kind === "explicit" && count < rowStore.totalCount}
        <button
          type="button"
          class="btn"
          disabled={selectingAll}
          onclick={() => void selectAll()}
        >
          {selectingAll ? "全选中…" : "全选"}
        </button>
      {/if}
      <button
        type="button"
        class="btn"
        onclick={() => (groupDialogOpen = true)}
      >
        分组
      </button>
      <button
        type="button"
        class="btn"
        onclick={() => (promptEditOpen = true)}
      >
        编辑提示词
      </button>
      <button
        type="button"
        class="btn btn-danger"
        onclick={() => requestDelete(selectionDto(), count)}
      >
        删除
      </button>
      <button type="button" class="btn btn-ghost" onclick={dismiss}>
        清除
      </button>
    </div>
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
  .selection-bar-wrap {
    position: absolute;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 40;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }

  .selection-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    box-shadow: var(--shadow-2);
    padding: 8px 10px 8px 16px;
    white-space: nowrap;
  }

  .count {
    font-size: 13px;
    font-weight: 600;
    margin-right: 4px;
  }

  .count small {
    font-weight: 400;
    font-size: 12px;
  }

  .selection-bar .btn {
    padding: 4px 12px;
    font-size: 12.5px;
    border-radius: 999px;
  }

</style>
