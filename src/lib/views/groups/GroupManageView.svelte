<script lang="ts">
  import { app, errorText } from "../../stores/app-state.svelte";
  import { cleanEmptyGroups, groupStore, loadGroups, removeGroup, renameExistingGroup } from "../../stores/group-store.svelte";
  import { resetRows } from "../../stores/row-store.svelte";

  let editingId = $state<number | null>(null);
  let editName = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    void loadGroups();
  });

  function close(): void {
    app.groupManageOpen = false;
  }

  function startRename(id: number, name: string): void {
    editingId = id;
    editName = name;
  }

  async function saveRename(): Promise<void> {
    if (editingId === null || busy) return;
    busy = true;
    error = null;
    try {
      await renameExistingGroup(editingId, editName);
      editingId = null;
      resetRows();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function doDelete(groupId: number): Promise<void> {
    if (busy) return;
    busy = true;
    error = null;
    try {
      await removeGroup(groupId);
      resetRows();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function doCleanEmpty(): Promise<void> {
    if (busy) return;
    busy = true;
    error = null;
    try {
      const count = await cleanEmptyGroups();
      if (count > 0) resetRows();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="overlay">
  <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
  <div class="panel" onclick={e => e.stopPropagation()}>
    <header>
      <h2>管理分组（{groupStore.list.length}）</h2>
      <div class="header-actions">
        <button type="button" class="btn" disabled={busy} onclick={() => void doCleanEmpty()}>清理空分组</button>
        <button type="button" class="close-btn" onclick={close}>&times;</button>
      </div>
    </header>

    {#if error}
      <p class="error">{error}</p>
    {/if}

    <div class="list">
      {#each groupStore.list as group (group.id)}
        <div class="group-row">
          {#if editingId === group.id}
            <input
              type="text"
              class="rename-input"
              bind:value={editName}
              disabled={busy}
              onkeydown={e => {
                if (e.key === "Enter") void saveRename();
                if (e.key === "Escape") editingId = null;
              }}
            />
            <button type="button" class="btn btn-sm" disabled={busy} onclick={() => (editingId = null)}>取消</button>
            <button type="button" class="btn btn-sm btn-primary" disabled={busy} onclick={() => void saveRename()}>保存</button>
          {:else}
            <span class="group-name">{group.name}</span>
            <span class="group-count">{group.memberCount} 行</span>
            <button type="button" class="btn btn-sm" onclick={() => startRename(group.id, group.name)}>重命名</button>
            <button type="button" class="btn btn-sm btn-danger" disabled={busy} onclick={() => void doDelete(group.id)}>删除</button>
          {/if}
        </div>
      {:else}
        <p class="empty">暂无分组</p>
      {/each}
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 70;
    background: rgb(15 20 28 / 50%);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .panel {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-2);
    width: 480px;
    max-width: 95vw;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }

  header h2 {
    font-size: 16px;
    font-weight: 600;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .close-btn {
    border: none;
    background: none;
    font-size: 20px;
    color: var(--text-2);
    cursor: pointer;
  }

  .error {
    padding: 8px 18px;
    font-size: 13px;
    color: var(--danger);
  }

  .list {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
    padding: 8px 18px 14px;
  }

  .group-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }

  .group-name {
    flex: 1;
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .group-count {
    font-size: 12px;
    color: var(--text-3);
    flex: none;
  }

  .rename-input {
    flex: 1;
    padding: 4px 8px;
    border: 1px solid var(--accent);
    border-radius: var(--radius-s);
    font-size: 13px;
  }

  .btn-sm {
    padding: 3px 8px;
    font-size: 12px;
  }

  .btn-primary {
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }

  .btn-danger {
    color: var(--danger);
  }

  .empty {
    padding: 24px 0;
    font-size: 13px;
    color: var(--text-3);
    text-align: center;
  }
</style>
