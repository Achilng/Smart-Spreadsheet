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
  import { softFade, softPop } from "../../ui/motion";

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
  <div class="selection-bar-wrap" transition:softPop={{ duration: 180, y: 10, start: 0.98 }}>
    <div class="selection-bar">
      <span class="count">
        已选
        {#key count}
          <span class="count-value" transition:softFade={{ duration: 110 }}>{formatCount(count)}</span>
        {/key}
        行
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
          transition:softFade={{ duration: 120 }}
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
    z-index: var(--z-nav);
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
    border-radius: var(--radius-full);
    box-shadow: var(--shadow-2);
    padding: 8px 10px 8px 16px;
    white-space: nowrap;
  }

  .count {
    font-size: var(--font-md);
    font-weight: 600;
    margin-right: 4px;
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

  .selection-bar .btn {
    padding: 4px 12px;
    font-size: var(--font-sm);
    border-radius: var(--radius-full);
  }

</style>
