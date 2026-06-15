<script lang="ts">
  import type { RowSelection } from "../../api";
  import { errorText } from "../app-state.svelte";
  import { assignToGroup, createNewGroup, groupStore, loadGroups, removeFromGroup } from "../group-store.svelte";
  import { resetRows } from "../row-store.svelte";

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
      const affected = await assignToGroup(selection, groupId);
      result = `已将 ${affected} 行分配到「${groupName}」`;
      resetRows();
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
    try {
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
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function ungroup(): Promise<void> {
    if (busy) return;
    busy = true;
    error = null;
    result = null;
    try {
      const affected = await removeFromGroup(selection);
      result = `已取消 ${affected} 行的分组`;
      resetRows();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onkeydown={e => { if (e.key === "Escape") onclose(); }} onclick={onclose}>
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_interactive_supports_focus -->
  <div class="dialog" role="dialog" aria-modal="true" onclick={e => e.stopPropagation()}>
    <header>
      <h3>分组操作（{count} 行）</h3>
      <button type="button" class="close-btn" onclick={onclose}>&times;</button>
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
        <p class="result-ok">{result}</p>
      {/if}
      {#if error}
        <p class="result-err">{error}</p>
      {/if}
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 80;
    background: rgb(15 20 28 / 50%);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dialog {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-2);
    width: 400px;
    max-width: 90vw;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
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
    font-size: 14px;
    font-weight: 600;
  }

  .close-btn {
    border: none;
    background: none;
    font-size: 18px;
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
    font-size: 13px;
  }

  .create-row input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .btn-primary {
    padding: 6px 12px;
    font-size: 12px;
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
    font-size: 12px;
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
    font-size: 13px;
    cursor: pointer;
  }

  .group-item:hover:not(:disabled) {
    background: var(--surface-2);
  }

  .group-count {
    font-size: 12px;
    color: var(--text-3);
  }

  .ungroup-btn {
    align-self: flex-start;
    padding: 4px 12px;
    font-size: 12px;
  }

  .result-ok {
    font-size: 13px;
    color: var(--success, #22c55e);
  }

  .result-err {
    font-size: 13px;
    color: var(--danger);
  }
</style>
