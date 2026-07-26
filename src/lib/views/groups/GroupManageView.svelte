<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import { flip } from "svelte/animate";

  import { app, errorText } from "../../stores/app-state.svelte";
  import { cleanEmptyGroups, groupStore, loadGroups, removeGroup, renameExistingGroup } from "../../stores/group-store.svelte";
  import { requestGroupDelete } from "../../stores/group-delete-confirm.svelte";
  import { resetRows } from "../../stores/row-store.svelte";
  import Modal from "../../ui/Modal.svelte";
  import { softFade } from "../../ui/motion";

  let editingId = $state<number | null>(null);
  let editName = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let cleanResult = $state<string | null>(null);

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
    if (editName.trim() === "") {
      error = "分组名不能为空";
      return;
    }
    busy = true;
    error = null;
    try {
      const ok = await renameExistingGroup(editingId, editName);
      if (!ok) {
        // store 层吞掉异常并把信息放进 groupStore.error，这里必须转出来展示
        error = groupStore.error ?? "重命名失败";
        return;
      }
      editingId = null;
      resetRows();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  function askDelete(groupId: number): void {
    const group = groupStore.list.find(g => g.id === groupId);
    if (!group) return;
    requestGroupDelete(group, async () => {
      error = null;
      const ok = await removeGroup(groupId);
      if (!ok) {
        error = groupStore.error ?? "删除分组失败";
        return;
      }
      resetRows();
    });
  }

  async function doCleanEmpty(): Promise<void> {
    if (busy) return;
    busy = true;
    error = null;
    cleanResult = null;
    try {
      const emptyCount = groupStore.list.filter(g => g.memberCount === 0).length;
      const count = await cleanEmptyGroups();
      if (count === 0 && groupStore.error) {
        error = groupStore.error;
        return;
      }
      cleanResult = count > 0 ? `已清理 ${count} 个空分组` : emptyCount === 0 ? "没有空分组需要清理" : "没有分组被清理";
      if (count > 0) resetRows();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal open={app.groupManageOpen} onclose={close} width="480px">
  <div class="panel">
    <header>
      <h2>管理分组（{groupStore.list.length}）</h2>
      <div class="header-actions">
        <button type="button" class="btn" disabled={busy} onclick={() => void doCleanEmpty()}>清理空分组</button>
        <button type="button" class="close-btn" onclick={close}><X size={15} strokeWidth={2} /></button>
      </div>
    </header>

    {#if error}
      <p class="error" transition:softFade={{ duration: 140 }}>{error}</p>
    {/if}
    {#if cleanResult}
      <p class="clean-result" transition:softFade={{ duration: 140 }}>{cleanResult}</p>
    {/if}

    <div class="list">
      {#each groupStore.list as group (group.id)}
        <div class="group-row" animate:flip={{ duration: 170 }} transition:softFade={{ duration: 130 }}>
          {#if editingId === group.id}
            <input
              type="text"
              class="rename-input"
              bind:value={editName}
              disabled={busy}
              onkeydown={e => {
                if (e.key === "Enter") void saveRename();
                if (e.key === "Escape") {
                  // 消费掉这次 Esc：只取消本次重命名，不关闭整个对话框。
                  e.preventDefault();
                  e.stopPropagation();
                  editingId = null;
                }
              }}
            />
            <button type="button" class="btn btn-sm" disabled={busy} onclick={() => (editingId = null)}>取消</button>
            <button type="button" class="btn btn-sm btn-primary" disabled={busy} onclick={() => void saveRename()}>保存</button>
          {:else}
            <span class="group-name">{group.name}</span>
            <span class="group-count">{group.memberCount} 行</span>
            <button type="button" class="btn btn-sm" onclick={() => startRename(group.id, group.name)}>重命名</button>
            <button type="button" class="btn btn-sm btn-danger" disabled={busy} onclick={() => askDelete(group.id)}>删除</button>
          {/if}
        </div>
      {:else}
        <p class="empty">暂无分组</p>
      {/each}
    </div>
  </div>
</Modal>

<style>
  .panel {
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
    font-size: var(--font-lg);
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
    font-size: var(--font-xl);
    color: var(--text-2);
    cursor: pointer;
  }

  .error {
    padding: 8px 18px;
    font-size: var(--font-md);
    color: var(--danger);
  }

  .clean-result {
    padding: 8px 18px;
    font-size: var(--font-md);
    color: var(--text-2);
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
    font-size: var(--font-md);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .group-count {
    font-size: var(--font-sm);
    color: var(--text-3);
    flex: none;
  }

  .rename-input {
    flex: 1;
    padding: 4px 8px;
    border: 1px solid var(--accent);
    border-radius: var(--radius-s);
    font-size: var(--font-md);
  }

  .btn-sm {
    padding: 3px 8px;
    font-size: var(--font-sm);
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
    font-size: var(--font-md);
    color: var(--text-3);
    text-align: center;
  }
</style>
