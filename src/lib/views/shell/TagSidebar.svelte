<script lang="ts">
  import {
    createTag,
    deleteTag,
    renameTag,
    type DedupeMode,
    type TagMatchMode,
  } from "../../api";
  import ContextMenuShell from "../../ui/ContextMenuShell.svelte";
  import Modal from "../../ui/Modal.svelte";
  import { app, bumpDataVersion, errorText, formatCount } from "../../stores/app-state.svelte";
  import { captureSelectionStates, restoreRowStates } from "../../stores/history-actions";
  import { recordHistory } from "../../stores/history.svelte";
  import {
    resetRows,
    rowStore,
    setDedupe,
    setFilter,
    setHasVibe,
    setHideGrouped,
    setSingleArtistOnly,
    setUntaggedOnly,
  } from "../../stores/row-store.svelte";
  import { clearSelection } from "../../stores/selection-store.svelte";
  import { loadTags, tagStore } from "../../stores/tag-store.svelte";
  import { softFade } from "../../ui/motion";
  import { tagColorFor } from "../../utils/tag-colors";

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
        search: "",
        excludedRowIds: [],
      });
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
      Number(rowStore.hideGrouped),
  );
  const filterSummary = $derived(
    activeTags.length === 0 && switchCount === 0
      ? "未启用筛选"
      : `${activeTags.length} 个 Tag · ${switchCount} 个开关生效`,
  );
</script>

<div class="tag-sidebar">
  <header class="sidebar-header">
    <div class="header-copy">
      <h3>筛选</h3>
      <p class="header-sub tabular">{filterSummary}</p>
    </div>
  </header>

  <div class="f-group" role="group" aria-label="去重与筛选">
    <div class="f-head">显示</div>
    <label class="check-row" class:on={rowStore.dedupe === "positivePrompt"} class:is-disabled={app.viewMode === "group"}>
      <input
        type="checkbox"
        checked={rowStore.dedupe === "positivePrompt"}
        disabled={app.viewMode === "group"}
        onchange={() => toggleDedupe("positivePrompt")}
      />
      <span class="cbox" aria-hidden="true"></span>
      按正向提示词去重
    </label>
    <label class="check-row" class:on={rowStore.dedupe === "artists"} class:is-disabled={app.viewMode === "group"}>
      <input
        type="checkbox"
        checked={rowStore.dedupe === "artists"}
        disabled={app.viewMode === "group"}
        onchange={() => toggleDedupe("artists")}
      />
      <span class="cbox" aria-hidden="true"></span>
      按画师串去重
    </label>
    <label class="check-row" class:on={rowStore.singleArtistOnly}>
      <input
        type="checkbox"
        checked={rowStore.singleArtistOnly}
        onchange={() => { setSingleArtistOnly(!rowStore.singleArtistOnly); clearSelection(); }}
      />
      <span class="cbox" aria-hidden="true"></span>
      筛选单画师串图片
    </label>
    <label class="check-row" class:on={rowStore.hasVibe}>
      <input
        type="checkbox"
        checked={rowStore.hasVibe}
        onchange={() => { setHasVibe(!rowStore.hasVibe); clearSelection(); }}
      />
      <span class="cbox" aria-hidden="true"></span>
      筛选存在 VIBE 的图片
    </label>
    <label class="check-row" class:on={rowStore.untaggedOnly}>
      <input
        type="checkbox"
        checked={rowStore.untaggedOnly}
        onchange={() => { setUntaggedOnly(!rowStore.untaggedOnly); clearSelection(); }}
      />
      <span class="cbox" aria-hidden="true"></span>
      筛选无 Tag 的图片
    </label>
    <label class="check-row" class:on={rowStore.hideGrouped} class:is-disabled={app.viewMode === "group"}>
      <input
        type="checkbox"
        checked={rowStore.hideGrouped}
        disabled={app.viewMode === "group"}
        onchange={() => { setHideGrouped(!rowStore.hideGrouped); clearSelection(); }}
      />
      <span class="cbox" aria-hidden="true"></span>
      隐藏已分组
    </label>
  </div>

  <div class="f-head tag-head">
    Tag
    <button
      type="button"
      class="mode-link"
      title="切换 Tag 筛选的组合方式"
      onclick={() => setMode(rowStore.tagMode === "and" ? "or" : "and")}
    >
      {rowStore.tagMode === "and" ? "AND 模式 ⌄" : "OR 模式 ⌄"}
    </button>
  </div>

  <div class="tag-list">
    {#if tagStore.error}
      <p class="list-note">Tag 列表加载失败：{tagStore.error}</p>
    {:else if entries.length === 0}
      <p class="list-note faint">还没有 Tag。选中图片后点“编辑 Tag”即可创建。</p>
    {:else}
      {#each entries as entry (entry.name)}
        {@const filterOn = activeTags.includes(entry.name)}
        {@const tone = tagColorFor(entry.name, tagStore.list)}
        <button
          type="button"
          class="tag-row check-row"
          class:on={filterOn}
          aria-pressed={filterOn}
          onclick={() => toggleFilterTag(entry.name)}
          oncontextmenu={(e) => onTagContextMenu(e, entry.name)}
        >
          <span class="cbox" aria-hidden="true"></span>
          <span
            class="tag-color-swatch"
            style:--tag-color={tone.background}
            title="画廊胶囊颜色 {tone.background}"
            aria-hidden="true"
          ></span>
          <span class="tag-name" title={entry.name}>{entry.name}</span>
          <span class="tag-count">{formatCount(entry.rowCount)}</span>
        </button>
      {/each}
    {/if}
  </div>

  {#if status}
    <p class="form-status" class:is-error={status.isError} role="status" transition:softFade={{ duration: 140 }}>{status.text}</p>
  {/if}
</div>

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
    <p>重命名 Tag「{renamingFrom}」：所有关联图片会自动跟随，可用 Ctrl+Z 撤销。自动规则里引用的旧名称不会随之更新。</p>
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
        将从 {formatCount(affected)} 行上移除该 Tag 关联。可用 Ctrl+Z 撤销。
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
  .tag-sidebar {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .sidebar-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    padding: 20px 16px 14px;
    flex: none;
  }

  .header-copy {
    min-width: 0;
  }

  .sidebar-header h3 {
    font-size: 19px;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: var(--text);
  }

  .header-sub {
    margin-top: 3px;
    font-size: var(--font-sm);
    color: var(--text-3);
  }

  /* ---- 小节头（显示 / Tag） ---- */
  .f-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 4px;
    font-size: var(--font-xs);
    font-weight: 700;
    color: var(--text);
    margin-bottom: 6px;
  }

  .f-group {
    padding: 0 12px 14px;
    flex: none;
  }

  .tag-head {
    padding: 0 16px;
    margin-bottom: 2px;
    flex: none;
  }

  .mode-link {
    border: none;
    background: none;
    font-size: 11px;
    font-weight: 400;
    color: var(--accent);
    padding: 0;
  }

  .mode-link:hover {
    text-decoration: underline;
  }

  /* ---- check-row（显示开关 + Tag 行共用） ---- */
  .check-row {
    display: flex;
    align-items: center;
    gap: 9px;
    min-height: 30px;
    width: 100%;
    border: none;
    background: none;
    border-radius: var(--radius-s);
    padding: 0 4px;
    font-size: var(--font-md);
    color: var(--text-2);
    text-align: left;
    cursor: pointer;
    transition:
      background var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive),
      transform var(--motion-press) var(--ease-responsive);
  }

  .check-row:hover:not(:disabled):not(.is-disabled) {
    color: var(--text);
  }

  .check-row.on {
    color: var(--text);
    font-weight: 600;
  }

  .check-row.is-disabled {
    opacity: 0.5;
    cursor: default;
  }

  /* label 内的原生 checkbox 隐藏，语义保留，视觉走自绘 cbox */
  .check-row input[type="checkbox"] {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
    pointer-events: none;
  }

  .cbox {
    width: 16px;
    height: 16px;
    border-radius: 5px;
    flex: none;
    border: 1.5px solid var(--border-strong);
    background: var(--surface);
    transition:
      background var(--motion-fast) var(--ease-responsive),
      border-color var(--motion-fast) var(--ease-responsive);
  }

  .check-row.on .cbox {
    background-color: var(--primary);
    background-image: url('data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M4 8.5 6.8 11 12 5.5" fill="none" stroke="white" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>');
    background-position: center;
    background-size: 12px;
    background-repeat: no-repeat;
    border-color: var(--primary);
  }

  .tag-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 12px 8px;
    display: flex;
    flex-direction: column;
    gap: 0;
  }

  .list-note {
    padding: 8px 6px;
    font-size: var(--font-sm);
  }

  .tag-row:active:not(:disabled) {
    transform: scale(0.995);
  }

  .tag-row:disabled {
    cursor: default;
    opacity: 0.6;
  }

  .tag-row {
    gap: 7px;
  }

  .tag-color-swatch {
    width: 11px;
    height: 11px;
    border-radius: 3px;
    background: var(--tag-color);
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 16%);
    flex: none;
  }

  .tag-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag-count {
    font-size: 11.5px;
    color: var(--text-4);
    font-variant-numeric: tabular-nums;
    flex: none;
  }

  .check-row.on .tag-count {
    color: var(--text-3);
  }

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
