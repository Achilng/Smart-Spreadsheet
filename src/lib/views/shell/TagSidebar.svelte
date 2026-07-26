<script lang="ts">
  import {
    addTagsToSelection,
    createTag,
    deleteTag,
    listSelectionTags,
    removeTagsFromSelection,
    type DedupeMode,
    type TagMatchMode,
    type TagSelectionSummary,
  } from "../../api";
  import ContextMenuShell from "../../ui/ContextMenuShell.svelte";
  import Modal from "../../ui/Modal.svelte";
  import { app, bumpDataVersion, errorText, formatCount } from "../../stores/app-state.svelte";
  import { captureSelectionStates, recordRowStateChange, restoreRowStates } from "../../stores/history-actions";
  import { recordHistory } from "../../stores/history.svelte";
  import {
    resetRows,
    rowStore,
    setDedupe,
    setFilter,
    setHasVibe,
    setHideGrouped,
    setSingleArtistOnly,
  } from "../../stores/row-store.svelte";
  import {
    clearSelection,
    getSelectedCount,
    materializeSelection,
    selection,
    selectionDto,
  } from "../../stores/selection-store.svelte";
  import { loadTags, tagStore } from "../../stores/tag-store.svelte";
  import { softFade, softFly } from "../../ui/motion";

  type SidebarMode = "filter" | "tag";
  type CoverageState = "all" | "partial" | "none";

  let sidebarMode = $state<SidebarMode>("filter");
  let newTagName = $state("");
  let tagging = $state(false);
  let coverageLoading = $state(false);
  let coverageError = $state<string | null>(null);
  let coverage = $state<TagSelectionSummary[]>([]);
  let status = $state<{ text: string; isError: boolean } | null>(null);
  let coverageGeneration = 0;

  const activeTags = $derived(rowStore.tags);
  const selectedCount = $derived(getSelectedCount());
  const selectedByTag = $derived.by(
    () => new Map(coverage.map(summary => [summary.name, summary.selectedRows])),
  );

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

  $effect(() => {
    void selection.version;
    const mode = sidebarMode;
    const count = selectedCount;
    if (mode !== "tag" || count === 0) {
      coverageGeneration += 1;
      coverage = [];
      coverageLoading = false;
      coverageError = null;
      return;
    }
    void refreshCoverage();
  });

  function setSidebarMode(mode: SidebarMode): void {
    if (sidebarMode !== mode) {
      sidebarMode = mode;
      status = null;
      newTagName = "";
    }
  }

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

  function clearFilter(): void {
    if (activeTags.length > 0) {
      setFilter([], rowStore.tagMode);
      clearSelection();
    }
  }

  function toggleDedupe(mode: Exclude<DedupeMode, "none">): void {
    setDedupe(rowStore.dedupe === mode ? "none" : mode);
    clearSelection();
  }

  function tagCoverage(name: string): CoverageState {
    const matched = selectedByTag.get(name) ?? 0;
    if (selectedCount > 0 && matched >= selectedCount) {
      return "all";
    }
    return matched > 0 ? "partial" : "none";
  }

  async function refreshCoverage(): Promise<void> {
    const request = ++coverageGeneration;
    coverageLoading = true;
    coverageError = null;
    try {
      if (selection.kind === "filtered") {
        await materializeSelection();
        return;
      }
      const summaries = await listSelectionTags(selectionDto());
      if (request === coverageGeneration) {
        coverage = summaries;
      }
    } catch (error) {
      if (request === coverageGeneration) {
        coverageError = errorText(error);
      }
    } finally {
      if (request === coverageGeneration) {
        coverageLoading = false;
      }
    }
  }

  async function toggleAssignment(name: string): Promise<void> {
    if (selectedCount === 0 || tagging || coverageLoading) {
      return;
    }
    tagging = true;
    status = null;
    try {
      const before = await captureSelectionStates(selectionDto());
      const remove = tagCoverage(name) === "all";
      const result = remove
        ? await removeTagsFromSelection(selectionDto(), [name])
        : await addTagsToSelection(selectionDto(), [name]);
      status = {
        text: `${remove ? "已解除" : "已贴上"} Tag"${name}"：处理 ${formatCount(result.affectedRows)} 行，变更 ${formatCount(result.associationsChanged)} 个关联。`,
        isError: false,
      };
      resetRows();
      await loadTags();
      await recordRowStateChange(`${remove ? "移除" : "添加"} Tag「${name}」`, before);
      await refreshCoverage();
    } catch (error) {
      status = { text: `打标失败：${errorText(error)}`, isError: true };
    } finally {
      tagging = false;
    }
  }

  async function createAndAttach(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const name = newTagName.trim();
    if (!name) {
      status = { text: "请输入一个非空 Tag。", isError: true };
      return;
    }
    if (selectedCount === 0) {
      status = { text: "请先选择要打标的行。", isError: true };
      return;
    }
    tagging = true;
    status = null;
    try {
      const before = await captureSelectionStates(selectionDto());
      const result = await addTagsToSelection(selectionDto(), [name]);
      newTagName = "";
      status = {
        text: `已将 Tag"${name}"贴到 ${formatCount(result.affectedRows)} 行，变更 ${formatCount(result.associationsChanged)} 个关联。`,
        isError: false,
      };
      resetRows();
      await loadTags();
      await recordRowStateChange(`添加 Tag「${name}」`, before);
      await refreshCoverage();
    } catch (error) {
      status = { text: `即建即贴失败：${errorText(error)}`, isError: true };
    } finally {
      tagging = false;
    }
  }

  let tagMenu = $state({ open: false, x: 0, y: 0, name: "" });
  let confirmingDelete = $state<string | null>(null);

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

  async function confirmDeleteTag(): Promise<void> {
    const name = confirmingDelete;
    if (!name) return;
    confirmingDelete = null;
    try {
      const before = await captureSelectionStates({
        kind: "filtered",
        tags: [name],
        tagMode: "and",
        dedupe: "none",
        singleArtistOnly: false,
        hasVibe: false,
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
            bumpDataVersion({ preserveScroll: true });
          },
        });
      } else {
        status = { text: `Tag"${name}"不存在。`, isError: true };
      }
    } catch (error) {
      status = { text: `删除 Tag 失败：${errorText(error)}`, isError: true };
    }
  }
</script>

<div class="tag-sidebar">
  <header class="sidebar-header">
    <h3>Tag 库</h3>
    <div class="mode-switch" role="group" aria-label="Tag 侧边栏模式">
      <button
        type="button"
        class:is-active={sidebarMode === "filter"}
        aria-pressed={sidebarMode === "filter"}
        onclick={() => setSidebarMode("filter")}
      >
        筛选
      </button>
      <button
        type="button"
        class:is-active={sidebarMode === "tag"}
        aria-pressed={sidebarMode === "tag"}
        onclick={() => setSidebarMode("tag")}
      >
        打标
      </button>
    </div>
  </header>

  {#if sidebarMode === "filter"}
    <div class="filter-info" in:softFly={{ duration: 145, x: -4, y: 0 }}>
      <span class="faint">{formatCount(rowStore.totalCount)} 条匹配</span>
      <div class="filter-mode" role="group" aria-label="筛选模式">
        <button
          type="button"
          class:is-active={rowStore.tagMode === "and"}
          aria-pressed={rowStore.tagMode === "and"}
          onclick={() => setMode("and")}
        >
          AND
        </button>
        <button
          type="button"
          class:is-active={rowStore.tagMode === "or"}
          aria-pressed={rowStore.tagMode === "or"}
          onclick={() => setMode("or")}
        >
          OR
        </button>
      </div>
      {#if activeTags.length > 0}
        <button type="button" class="link-btn" onclick={clearFilter}>清除</button>
      {/if}
    </div>

    <div class="dedupe-options" role="group" aria-label="去重与筛选" in:softFly={{ duration: 145, x: -4, y: 0 }}>
      <label>
        <input
          type="checkbox"
          checked={rowStore.dedupe === "positivePrompt"}
          disabled={app.viewMode === "group"}
          onchange={() => toggleDedupe("positivePrompt")}
        />
        按正向提示词去重
      </label>
      <label>
        <input
          type="checkbox"
          checked={rowStore.dedupe === "artists"}
          disabled={app.viewMode === "group"}
          onchange={() => toggleDedupe("artists")}
        />
        按画师串去重
      </label>
      <label>
        <input
          type="checkbox"
          checked={rowStore.singleArtistOnly}
          onchange={() => { setSingleArtistOnly(!rowStore.singleArtistOnly); clearSelection(); }}
        />
        筛选单画师串图片
      </label>
      <label>
        <input
          type="checkbox"
          checked={rowStore.hasVibe}
          onchange={() => { setHasVibe(!rowStore.hasVibe); clearSelection(); }}
        />
        筛选存在 VIBE 的图片
      </label>
      <label>
        <input
          type="checkbox"
          checked={rowStore.hideGrouped}
          disabled={app.viewMode === "group"}
          onchange={() => { setHideGrouped(!rowStore.hideGrouped); clearSelection(); }}
        />
        隐藏已分组
      </label>
    </div>
  {:else}
    <div class="tagging-info" in:softFly={{ duration: 145, x: 4, y: 0 }}>
      {#if selectedCount === 0}
        <span>请先在画廊或表格中选择要打标的行。</span>
      {:else if coverageLoading}
        <span>正在准备 {formatCount(selectedCount)} 行的 Tag 状态…</span>
      {:else}
        <span>面向已选 {formatCount(selectedCount)} 行，点击 Tag 可贴上或解除。</span>
      {/if}
      {#if coverageError}
        <span class="is-error">加载覆盖状态失败：{coverageError}</span>
      {/if}
    </div>
  {/if}

  <div class="tag-list">
    {#if tagStore.error}
      <p class="list-note">Tag 列表加载失败：{tagStore.error}</p>
    {:else if entries.length === 0}
      <p class="list-note faint">
        {sidebarMode === "filter" ? "还没有 Tag。切到打标模式可即建即贴。" : "输入新名字即可创建并贴到所选行。"}
      </p>
    {:else}
      {#each entries as entry (entry.name)}
        {@const state = tagCoverage(entry.name)}
        <button
          type="button"
          class="tag-row"
          class:is-active={sidebarMode === "filter" && activeTags.includes(entry.name)}
          class:is-all={sidebarMode === "tag" && state === "all"}
          class:is-partial={sidebarMode === "tag" && state === "partial"}
          aria-pressed={
            sidebarMode === "filter" ? activeTags.includes(entry.name) : state === "all"
          }
          disabled={sidebarMode === "tag" && (selectedCount === 0 || coverageLoading || tagging)}
          onclick={() =>
            sidebarMode === "filter"
              ? toggleFilterTag(entry.name)
              : void toggleAssignment(entry.name)}
          oncontextmenu={(e) => onTagContextMenu(e, entry.name)}
        >
          <span class="tag-name" title={entry.name}>{entry.name}</span>
          {#if sidebarMode === "filter"}
            <span class="tag-count">{formatCount(entry.rowCount)}</span>
          {:else}
            <span class="coverage" class:is-partial={state === "partial"}>
              {state === "all" ? "全有" : state === "partial" ? "部分" : "无"}
            </span>
          {/if}
        </button>
      {/each}
    {/if}
  </div>

  {#if sidebarMode === "tag"}
    <form class="create-form" onsubmit={createAndAttach} transition:softFly={{ duration: 155, y: 6 }}>
      <input
        type="text"
        placeholder="输入 Tag 名称，即建即贴"
        bind:value={newTagName}
        disabled={tagging || coverageLoading || selectedCount === 0}
        autocomplete="off"
      />
      <button
        type="submit"
        class="btn"
        disabled={tagging || coverageLoading || selectedCount === 0}
      >
        贴上
      </button>
    </form>
  {/if}
  {#if status}
    <p class="form-status" class:is-error={status.isError} role="status" transition:softFade={{ duration: 140 }}>{status.text}</p>
  {/if}
</div>

<ContextMenuShell open={tagMenu.open} x={tagMenu.x} y={tagMenu.y} onclose={closeTagMenu}>
  <button type="button" role="menuitem" class="danger" onclick={requestDeleteTag}>
    删除 Tag "{tagMenu.name}"
  </button>
</ContextMenuShell>

<Modal open={confirmingDelete !== null} onclose={() => (confirmingDelete = null)} width="400px">
  <div class="confirm-dialog" aria-label="确认删除">
    <p>确定删除 Tag「{confirmingDelete}」吗？所有行上的该 Tag 关联将被同时移除。</p>
    <div class="confirm-actions">
      <button type="button" class="btn" onclick={() => (confirmingDelete = null)}>取消</button>
      <button type="button" class="btn btn-danger" onclick={() => void confirmDeleteTag()}>删除</button>
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
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    flex: none;
  }

  .sidebar-header h3 {
    font-size: var(--font-lg);
    font-weight: 650;
    letter-spacing: -0.01em;
    color: var(--text);
  }

  .mode-switch,
  .filter-mode {
    display: flex;
    background: var(--surface-3);
    border-radius: var(--radius-s);
    padding: 2px;
    gap: 2px;
  }

  .mode-switch button,
  .filter-mode button {
    border: none;
    background: transparent;
    border-radius: var(--radius-s);
    padding: 1px 8px;
    font-size: var(--font-xs);
    color: var(--text-2);
    transition:
      background var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive),
      box-shadow var(--motion-fast) var(--ease-responsive),
      transform var(--motion-press) var(--ease-responsive);
  }

  .mode-switch button:active,
  .filter-mode button:active {
    transform: scale(0.96);
  }

  .mode-switch button.is-active,
  .filter-mode button.is-active {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow-1);
  }

  .filter-info {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px 8px;
    font-size: var(--font-sm);
    flex: none;
  }

  .filter-info > span {
    margin-right: auto;
  }

  .link-btn {
    border: none;
    background: none;
    color: var(--accent);
    font-size: var(--font-sm);
    padding: 0;
  }

  .link-btn:hover {
    text-decoration: underline;
  }

  .dedupe-options {
    display: grid;
    gap: 5px;
    padding: 0 12px 10px;
    font-size: var(--font-sm);
    color: var(--text-2);
    flex: none;
  }

  .dedupe-options label {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }

  .dedupe-options input {
    margin: 0;
    accent-color: var(--accent);
  }

  .tagging-info {
    display: grid;
    gap: 4px;
    padding: 0 12px 10px;
    color: var(--text-2);
    font-size: var(--font-sm);
    line-height: 1.45;
  }

  .tagging-info .is-error {
    color: var(--danger);
  }

  .tag-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px 8px 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .list-note {
    padding: 8px 6px;
    font-size: var(--font-sm);
  }

  .tag-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    border: none;
    background: none;
    border-radius: var(--radius-s);
    padding: 5px 8px;
    font-size: var(--font-md);
    text-align: left;
    color: var(--text);
    transition:
      background var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive),
      transform var(--motion-press) var(--ease-responsive);
  }

  .tag-row:active:not(:disabled) {
    transform: scale(0.99);
  }

  .tag-row:hover:not(:disabled) {
    background: var(--surface-2);
  }

  .tag-row.is-active,
  .tag-row.is-all {
    background: var(--accent-soft);
    color: var(--accent);
  }

  .tag-row.is-partial {
    background: var(--surface-2);
  }

  .tag-row:disabled {
    cursor: default;
    opacity: 0.6;
  }

  .tag-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag-count,
  .coverage {
    font-size: var(--font-xs);
    color: var(--text-3);
    flex: none;
  }

  .tag-row.is-active .tag-count,
  .tag-row.is-all .coverage {
    color: var(--accent);
  }

  .coverage.is-partial {
    color: var(--warning, #a56700);
  }

  .create-form {
    display: flex;
    gap: 6px;
    padding: 10px 12px 4px;
    flex: none;
  }

  .create-form input {
    flex: 1;
    min-width: 0;
    padding: 5px 9px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    font-size: var(--font-sm);
  }

  .create-form input:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: var(--focus-ring);
  }

  .create-form .btn {
    padding: 4px 10px;
    font-size: var(--font-sm);
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
    box-shadow: var(--shadow-3);
    padding: 20px 24px;
    max-width: 400px;
    width: 90vw;
  }

  .confirm-dialog p {
    margin: 0 0 16px;
    font-size: var(--font-base);
    line-height: 1.5;
  }

  .confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .confirm-actions .btn-danger {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
  }

  .confirm-actions .btn-danger:hover {
    opacity: 0.9;
  }
</style>
