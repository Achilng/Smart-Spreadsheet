<script lang="ts">
  import { bumpDataVersion } from "../../stores/library-changes";
  import { errorText, formatCount } from "../../utils/format";
  import { workspaceState } from "../../stores/workspace-state.svelte";
  import { materialIdsForTag, restoreMaterialTag } from "../../api/materials";
  import {
    createTag,
    deleteTag,
    renameTag,
    type DedupeMode,
    type TagMatchMode,
  } from "../../api";
  import ListFilter from "@lucide/svelte/icons/list-filter";
  import ContextMenuShell from "../../ui/ContextMenuShell.svelte";
  import Modal from "../../ui/Modal.svelte";

  import { captureSelectionStates, restoreRowStates } from "../../stores/history-actions";
  import { recordHistory } from "../../stores/history.svelte";
  import {
    resetRows,
    rowStore,
    setDedupe,
    setFilter,
  } from "../../stores/row-store.svelte";
  import { clearSelection } from "../../stores/selection-store.svelte";
  import { loadTags, tagStore } from "../../stores/tag-store.svelte";
  import { softFade } from "../../ui/motion";
  import TagFilterSidebar from "../../ui/TagFilterSidebar.svelte";

  let status = $state<{ text: string; isError: boolean } | null>(null);

  const activeTags = $derived(rowStore.tags);

  /** Tag 库列表 + 仍在筛选中但已被自动清理的 Tag（计数 0） */
  const entries = $derived.by(() => {
    const known = new Set(tagStore.list.map(tag => tag.name));
    const merged = tagStore.list.map(tag => ({ name: tag.name, rowCount: tag.rowCount }));
    for (const active of activeTags) {
      if (!known.has(active)) {
        merged.push({ name: active, rowCount: 0 });
      }
    }
    return merged;
  });

  function toggleFilterTag(name: string): void {
    const next = activeTags.includes(name)
      ? activeTags.filter(tag => tag !== name)
      : [...activeTags, name];
    setFilter(next, rowStore.tagMode);
    clearSelection();
  }

  function setMode(mode: TagMatchMode): void {
    if (mode !== rowStore.tagMode) {
      setFilter(activeTags, mode);
      clearSelection();
    }
  }

  function toggleDedupe(mode: Exclude<DedupeMode, "none">): void {
    setDedupe(rowStore.dedupe === mode ? "none" : mode);
    clearSelection();
  }

  let tagMenu = $state({ open: false, x: 0, y: 0, name: "" });
  let confirmingDelete = $state<string | null>(null);
  let deletingTag = $state(false);
  let renamingFrom = $state<string | null>(null);
  let renameTo = $state("");
  let renamingTag = $state(false);

  function onTagContextMenu(event: MouseEvent, name: string): void {
    event.preventDefault();
    tagMenu = { open: true, x: event.clientX, y: event.clientY, name };
  }

  function closeTagMenu(): void {
    tagMenu.open = false;
  }

  function requestDeleteTag(): void {
    confirmingDelete = tagMenu.name;
    closeTagMenu();
  }

  function requestRenameTag(): void {
    renamingFrom = tagMenu.name;
    renameTo = tagMenu.name;
    closeTagMenu();
  }

  /** 筛选条件里引用旧名时同步替换为新名 */
  function renameInActiveFilter(oldName: string, newName: string): void {
    if (rowStore.tags.includes(oldName)) {
      setFilter(
        rowStore.tags.map(tag => (tag === oldName ? newName : tag)),
        rowStore.tagMode,
      );
    }
  }

  async function confirmRenameTag(): Promise<void> {
    const oldName = renamingFrom;
    const newName = renameTo.trim();
    if (!oldName || renamingTag) return;
    if (!newName) {
      status = { text: "新名称不能为空。", isError: true };
      return;
    }
    if (newName === oldName) {
      renamingFrom = null;
      return;
    }
    renamingTag = true;
    try {
      const renamed = await renameTag(oldName, newName);
      if (renamed) {
        renameInActiveFilter(oldName, newName);
        await loadTags();
        bumpDataVersion({ preserveScroll: true, preserveSelection: true });
        status = { text: `已将 Tag"${oldName}"重命名为"${newName}"。`, isError: false };
        recordHistory({
          label: `重命名 Tag「${oldName}」为「${newName}」`,
          undo: async () => {
            await renameTag(newName, oldName);
            renameInActiveFilter(newName, oldName);
            await loadTags();
            bumpDataVersion({ preserveScroll: true, preserveSelection: true });
          },
          redo: async () => {
            await renameTag(oldName, newName);
            renameInActiveFilter(oldName, newName);
            await loadTags();
            bumpDataVersion({ preserveScroll: true, preserveSelection: true });
          },
        });
      } else {
        status = { text: `Tag"${oldName}"不存在。`, isError: true };
      }
      renamingFrom = null;
    } catch (error) {
      status = { text: `重命名失败：${errorText(error)}`, isError: true };
    } finally {
      renamingTag = false;
    }
  }

  async function confirmDeleteTag(): Promise<void> {
    const name = confirmingDelete;
    if (!name || deletingTag) return;
    // 保持对话框可见并置 busy：捕获行状态在大库上可能耗时数秒
    deletingTag = true;
    try {
      const before = await captureSelectionStates({
        kind: "filtered",
        tags: [name],
        tagMode: "and",
        dedupe: "none",
        singleArtistOnly: false,
        artistFilter: "",
        hasVibe: false,
        untaggedOnly: false,
        filters: [],
        search: "",
        excludedRowIds: [],
      });
      const materialIds = await materialIdsForTag(name);
      const deleted = await deleteTag(name);
      if (deleted) {
        if (activeTags.includes(name)) {
          setFilter(
            activeTags.filter((t) => t !== name),
            rowStore.tagMode,
          );
          clearSelection();
        }
        resetRows();
        await loadTags();
        status = { text: `已删除 Tag"${name}"。`, isError: false };
        recordHistory({
          label: `删除 Tag「${name}」`,
          undo: async () => {
            await createTag(name);
            await restoreMaterialTag(name, materialIds);
            if (before.length > 0) {
              await restoreRowStates(before);
            } else {
              await loadTags();
            }
          },
          redo: async () => {
            await deleteTag(name);
            await loadTags();
            bumpDataVersion({ preserveScroll: true, preserveSelection: true });
          },
        });
      } else {
        status = { text: `Tag"${name}"不存在。`, isError: true };
      }
    } catch (error) {
      status = { text: `删除 Tag 失败：${errorText(error)}`, isError: true };
    } finally {
      deletingTag = false;
      confirmingDelete = null;
    }
  }
  const switchCount = $derived(
    Number(rowStore.dedupe !== "none") +
      Number(rowStore.singleArtistOnly) +
      Number(rowStore.hasVibe) +
      Number(rowStore.untaggedOnly) +
      Number(rowStore.hideGrouped) +
      rowStore.filters.length,
  );
  const filterSummary = $derived(
    activeTags.length === 0 && switchCount === 0
      ? "未启用筛选"
      : `${activeTags.length} 个 Tag · ${switchCount} 个条件生效`,
  );
</script>

<TagFilterSidebar {entries} {activeTags} summary={filterSummary} ontoggle={toggleFilterTag}
  oncontextmenu={onTagContextMenu} modeLabel={rowStore.tagMode === "and" ? "AND 模式 ⌄" : "OR 模式 ⌄"}
  onmode={() => setMode(rowStore.tagMode === "and" ? "or" : "and")} error={tagStore.error}>
  {#snippet filters()}
  <div class="f-group" role="group" aria-label="去重与筛选">
    <div class="f-head">显示</div>
    <label class="check-row" class:on={rowStore.dedupe === "positivePrompt"} class:is-disabled={workspaceState.viewMode === "group"}>
      <input
        type="checkbox"
        checked={rowStore.dedupe === "positivePrompt"}
        disabled={workspaceState.viewMode === "group"}
        onchange={() => toggleDedupe("positivePrompt")}
      />
      <span class="cbox" aria-hidden="true"></span>
      按正向提示词去重
    </label>
    <label class="check-row" class:on={rowStore.dedupe === "artists"} class:is-disabled={workspaceState.viewMode === "group"}>
      <input
        type="checkbox"
        checked={rowStore.dedupe === "artists"}
        disabled={workspaceState.viewMode === "group"}
        onchange={() => toggleDedupe("artists")}
      />
      <span class="cbox" aria-hidden="true"></span>
      按画师串去重
    </label>
    <button
      type="button"
      class="filter-launch"
      class:on={rowStore.filters.length > 0}
      onclick={() => (workspaceState.filterOpen = true)}
    >
      <ListFilter size={16} strokeWidth={1.9} />
      <span>过滤</span>
      {#if rowStore.filters.length > 0}
        <span class="filter-badge tabular">{rowStore.filters.length}</span>
      {:else}
        <span class="filter-hint">选择条件</span>
      {/if}
    </button>
  </div>

  {/snippet}
  {#snippet statusContent()}
  {#if status}
    <p class="form-status" class:is-error={status.isError} role="status" transition:softFade={{ duration: 140 }}>{status.text}</p>
  {/if}
  {/snippet}
</TagFilterSidebar>

<ContextMenuShell open={tagMenu.open} x={tagMenu.x} y={tagMenu.y} onclose={closeTagMenu}>
  <button type="button" role="menuitem" onclick={requestRenameTag}>
    重命名 Tag "{tagMenu.name}"
  </button>
  <button type="button" role="menuitem" class="danger" onclick={requestDeleteTag}>
    删除 Tag "{tagMenu.name}"
  </button>
</ContextMenuShell>

<Modal
  open={renamingFrom !== null}
  onclose={() => { if (!renamingTag) renamingFrom = null; }}
  busy={renamingTag}
  width="400px"
>
  <div class="confirm-dialog" aria-label="重命名 Tag">
    <p>重命名 Tag「{renamingFrom}」：所有关联图片和素材会自动跟随，可用 Ctrl+Z 撤销。自动规则里引用的旧名称不会随之更新。</p>
    <input
      type="text"
      class="rename-input"
      bind:value={renameTo}
      disabled={renamingTag}
      autocomplete="off"
      onkeydown={e => { if (e.key === "Enter") void confirmRenameTag(); }}
    />
    <div class="confirm-actions">
      <button type="button" class="btn" disabled={renamingTag} onclick={() => (renamingFrom = null)}>取消</button>
      <button
        type="button"
        class="btn btn-primary"
        disabled={renamingTag || renameTo.trim() === ""}
        onclick={() => void confirmRenameTag()}
      >
        {renamingTag ? "重命名中…" : "重命名"}
      </button>
    </div>
  </div>
</Modal>

<Modal
  open={confirmingDelete !== null}
  onclose={() => { if (!deletingTag) confirmingDelete = null; }}
  busy={deletingTag}
  width="400px"
>
  <div class="confirm-dialog" aria-label="确认删除">
    <p>
      确定删除 Tag「{confirmingDelete}」吗？
      {#if confirmingDelete}
        {@const affected = entries.find(e => e.name === confirmingDelete)?.rowCount ?? 0}
        将从 {formatCount(affected)} 行以及使用此 Tag 的素材上移除关联。可用 Ctrl+Z 撤销。
      {/if}
    </p>
    <div class="confirm-actions">
      <button type="button" class="btn" disabled={deletingTag} onclick={() => (confirmingDelete = null)}>取消</button>
      <button type="button" class="btn btn-danger" disabled={deletingTag} onclick={() => void confirmDeleteTag()}>
        {deletingTag ? "删除中…" : "删除"}
      </button>
    </div>
  </div>
</Modal>

<style>
  .form-status {
    padding: 2px 12px 8px;
    font-size: var(--font-sm);
    color: var(--success);
  }

  .form-status.is-error {
    color: var(--danger);
  }

  .confirm-dialog {
    background: var(--surface);
    border-radius: var(--radius-l);
    padding: 20px 24px;
    max-width: 400px;
    width: 90vw;
  }

  .confirm-dialog p {
    margin: 0 0 16px;
    font-size: var(--font-base);
    line-height: 1.5;
  }

  .rename-input {
    width: 100%;
    margin-bottom: 16px;
  }

  .confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
