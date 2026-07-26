<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import { flip } from "svelte/animate";

  import type { RowSelection } from "../../api";
  import Modal from "../../ui/Modal.svelte";
  import { errorText } from "../../stores/app-state.svelte";
  import { captureSelectionStates, recordRowStateChange } from "../../stores/history-actions";
  import { beginHistoryGroup, commitHistoryGroup } from "../../stores/history.svelte";
  import { assignToGroup, createNewGroup, groupStore, loadGroups, removeFromGroup } from "../../stores/group-store.svelte";
  import { resetRows } from "../../stores/row-store.svelte";
  import { softFade } from "../../ui/motion";

  interface Props {
    selection: RowSelection;
    count: number;
    onclose: () => void;
  }

  let { selection, count, onclose }: Props = $props();

  let newGroupName = $state("");
  let busy = $state(false);
  let result = $state<string | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    void loadGroups();
  });

  async function assignTo(groupId: number, groupName: string): Promise<void> {
    if (busy) return;
    busy = true;
    error = null;
    result = null;
    try {
      const before = await captureSelectionStates(selection);
      const affected = await assignToGroup(selection, groupId);
      result = `已将 ${affected} 行分配到「${groupName}」`;
      resetRows();
      await recordRowStateChange(`分配到分组「${groupName}」`, before);
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function createAndAssign(): Promise<void> {
    const name = newGroupName.trim();
    if (busy || !name) return;
    busy = true;
    error = null;
    result = null;
    let historyGroupStarted = false;
    try {
      const before = await captureSelectionStates(selection);
      historyGroupStarted = beginHistoryGroup(`新建并分配分组「${name}」`);
      const group = await createNewGroup(name);
      if (!group) {
        error = groupStore.error ?? "创建分组失败";
        busy = false;
        return;
      }
      const affected = await assignToGroup(selection, group.id);
      result = `已创建「${group.name}」并分配 ${affected} 行`;
      newGroupName = "";
      resetRows();
      await recordRowStateChange(`分配到分组「${group.name}」`, before);
    } catch (e) {
      error = errorText(e);
    } finally {
      if (historyGroupStarted) {
        commitHistoryGroup();
      }
      busy = false;
    }
  }

  async function ungroup(): Promise<void> {
    if (busy) return;
    busy = true;
    error = null;
    result = null;
    try {
      const before = await captureSelectionStates(selection);
      const affected = await removeFromGroup(selection);
      result = `已取消 ${affected} 行的分组`;
      resetRows();
      await recordRowStateChange("取消分组", before);
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal open={true} {onclose} width="400px">
  <div class="dialog-content">
    <header>
      <h3>分组操作（{count} 行）</h3>
      <button type="button" class="close-btn" onclick={onclose}><X size={15} strokeWidth={2} /></button>
    </header>

    <div class="body">
      <div class="create-row">
        <input
          type="text"
          bind:value={newGroupName}
          placeholder="新分组名称…"
          disabled={busy}
          onkeydown={e => { if (e.key === "Enter") void createAndAssign(); }}
        />
        <button
          type="button"
          class="btn btn-primary"
          disabled={busy || !newGroupName.trim()}
          onclick={() => void createAndAssign()}
        >
          新建并分配
        </button>
      </div>

      {#if groupStore.list.length > 0}
        <div class="group-list">
          <h4>分配到已有分组</h4>
          {#each groupStore.list as group (group.id)}
            <button
              type="button"
              class="group-item"
              disabled={busy}
              onclick={() => void assignTo(group.id, group.name)}
              animate:flip={{ duration: 160 }}
            >
              <span class="group-name">{group.name}</span>
              <span class="group-count">{group.memberCount} 行</span>
            </button>
          {/each}
        </div>
      {/if}

      <button type="button" class="btn ungroup-btn" disabled={busy} onclick={() => void ungroup()}>
        取消分组
      </button>

      {#if result}
        <p class="result-ok" transition:softFade={{ duration: 140 }}>{result}</p>
      {/if}
      {#if error}
        <p class="result-err" transition:softFade={{ duration: 140 }}>{error}</p>
      {/if}
    </div>
  </div>
</Modal>

<style>
  .dialog-content {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }

  header h3 {
    font-size: var(--font-base);
    font-weight: 600;
  }

  .close-btn {
    border: none;
    background: none;
    font-size: var(--font-xl);
    color: var(--text-2);
    padding: 0 4px;
    cursor: pointer;
  }

  .body {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow-y: auto;
    min-height: 0;
  }

  .create-row {
    display: flex;
    gap: 8px;
  }

  .create-row input {
    flex: 1;
    padding: 6px 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    background: var(--surface);
    font-size: var(--font-md);
  }

  .create-row input:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: var(--focus-ring);
  }

  .btn-primary {
    padding: 6px 12px;
    font-size: var(--font-sm);
    background: var(--accent);
    color: white;
    border: 1px solid var(--accent);
    border-radius: var(--radius-s);
    cursor: pointer;
    white-space: nowrap;
  }

  .btn-primary:hover:not(:disabled) {
    filter: brightness(1.1);
  }

  .btn-primary:disabled {
    opacity: 0.5;
  }

  .group-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .group-list h4 {
    font-size: var(--font-sm);
    color: var(--text-2);
    font-weight: 600;
    margin-bottom: 4px;
  }

  .group-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    width: 100%;
    border: none;
    background: transparent;
    border-radius: var(--radius-s);
    padding: 6px 10px;
    font-size: var(--font-md);
    cursor: pointer;
    transition:
      background var(--motion-fast) var(--ease-responsive),
      transform var(--motion-press) var(--ease-responsive);
  }

  .group-item:active:not(:disabled) {
    transform: scale(0.985);
  }

  .group-item:hover:not(:disabled) {
    background: var(--surface-2);
  }

  .group-count {
    font-size: var(--font-sm);
    color: var(--text-3);
  }

  .ungroup-btn {
    align-self: flex-start;
    padding: 4px 12px;
    font-size: var(--font-sm);
  }

  .result-ok {
    font-size: var(--font-md);
    color: var(--success, #22c55e);
  }

  .result-err {
    font-size: var(--font-md);
    color: var(--danger);
  }
</style>
