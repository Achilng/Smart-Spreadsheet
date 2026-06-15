<script lang="ts">
  import {
    addTagsToSelection,
    deleteTag,
    listSelectionTags,
    removeTagsFromSelection,
    type DedupeMode,
    type TagMatchMode,
    type TagSelectionSummary,
  } from "../../api";
  import { errorText, formatCount } from "../app-state.svelte";
  import { resetRows, rowStore, setDedupe, setFilter, setGroupView, setHideGrouped, setSingleArtistOnly } from "../row-store.svelte";
  import {
    clearSelection,
    getSelectedCount,
    materializeSelection,
    selection,
    selectionDto,
  } from "../selection-store.svelte";
  import { loadTags, tagStore } from "../tag-store.svelte";

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
      const result = await addTagsToSelection(selectionDto(), [name]);
      newTagName = "";
      status = {
        text: `已将 Tag"${name}"贴到 ${formatCount(result.affectedRows)} 行，变更 ${formatCount(result.associationsChanged)} 个关联。`,
        isError: false,
      };
      resetRows();
      await loadTags();
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
      } else {
        status = { text: `Tag"${name}"不存在。`, isError: true };
      }
    } catch (error) {
      status = { text: `删除 Tag 失败：${errorText(error)}`, isError: true };
    }
  }
</script>

<svelte:window
  onpointerdown={() => {
    if (tagMenu.open) closeTagMenu();
  }}
/>


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
    <div class="filter-info">
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

    <div class="dedupe-options" role="group" aria-label="去重与分组">
      <label>
        <input
          type="checkbox"
          checked={rowStore.dedupe === "positivePrompt"}
          disabled={rowStore.groupView}
          onchange={() => toggleDedupe("positivePrompt")}
        />
        按正向提示词去重
      </label>
      <label>
        <input
          type="checkbox"
          checked={rowStore.dedupe === "artists"}
          disabled={rowStore.groupView}
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
          checked={rowStore.groupView}
          onchange={() => { setGroupView(!rowStore.groupView); clearSelection(); }}
        />
        分组视图
      </label>
      <label>
        <input
          type="checkbox"
          checked={rowStore.hideGrouped}
          disabled={rowStore.groupView}
          onchange={() => { setHideGrouped(!rowStore.hideGrouped); clearSelection(); }}
        />
        隐藏已分组
      </label>
    </div>
  {:else}
    <div class="tagging-info">
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
    <form class="create-form" onsubmit={createAndAttach}>
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
    <p class="form-status" class:is-error={status.isError} role="status">{status.text}</p>
  {/if}
</div>

{#if tagMenu.open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="tag-context-menu"
    role="menu"
    style:left="{tagMenu.x}px"
    style:top="{tagMenu.y}px"
    onpointerdown={(e) => e.stopPropagation()}
  >
    <button type="button" role="menuitem" class="danger" onclick={requestDeleteTag}>
      删除 Tag "{tagMenu.name}"
    </button>
  </div>
{/if}

{#if confirmingDelete}
  <div class="confirm-overlay" role="dialog" aria-label="确认删除">
    <div class="confirm-dialog">
      <p>确定删除 Tag「{confirmingDelete}」吗？所有行上的该 Tag 关联将被同时移除。</p>
      <div class="confirm-actions">
        <button type="button" class="btn" onclick={() => (confirmingDelete = null)}>取消</button>
        <button type="button" class="btn btn-danger" onclick={() => void confirmDeleteTag()}>删除</button>
      </div>
    </div>
  </div>
{/if}

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
    font-size: 13px;
    font-weight: 600;
  }

  .mode-switch,
  .filter-mode {
    display: flex;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    padding: 2px;
    gap: 2px;
  }

  .mode-switch button,
  .filter-mode button {
    border: none;
    background: transparent;
    border-radius: 4px;
    padding: 1px 8px;
    font-size: 11px;
    color: var(--text-2);
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
    font-size: 12px;
    flex: none;
  }

  .filter-info > span {
    margin-right: auto;
  }

  .link-btn {
    border: none;
    background: none;
    color: var(--accent);
    font-size: 12px;
    padding: 0;
  }

  .link-btn:hover {
    text-decoration: underline;
  }

  .dedupe-options {
    display: grid;
    gap: 5px;
    padding: 0 12px 10px;
    border-bottom: 1px solid var(--border);
    font-size: 12px;
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
    border-bottom: 1px solid var(--border);
    color: var(--text-2);
    font-size: 12px;
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
    font-size: 12px;
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
    font-size: 13px;
    text-align: left;
    color: var(--text);
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
    font-size: 11px;
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
    border-top: 1px solid var(--border);
    flex: none;
  }

  .create-form input {
    flex: 1;
    min-width: 0;
    padding: 5px 9px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    font-size: 12.5px;
  }

  .create-form input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .create-form .btn {
    padding: 4px 10px;
    font-size: 12.5px;
  }

  .form-status {
    padding: 2px 12px 8px;
    font-size: 11.5px;
    color: var(--success);
  }

  .form-status.is-error {
    color: var(--danger);
  }

  .tag-context-menu {
    position: fixed;
    z-index: 9999;
    min-width: 160px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    box-shadow: var(--shadow-2);
  }

  .tag-context-menu button {
    display: flex;
    align-items: center;
    width: 100%;
    border: none;
    background: transparent;
    border-radius: 4px;
    padding: 6px 10px;
    text-align: left;
    font-size: 13px;
    color: var(--text);
    cursor: default;
  }

  .tag-context-menu button:hover {
    background: var(--surface-2);
  }

  .tag-context-menu button.danger {
    color: var(--danger);
  }

  .confirm-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    z-index: 10000;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .confirm-dialog {
    background: var(--surface);
    border-radius: var(--radius-l, 12px);
    box-shadow: var(--shadow-3, 0 8px 32px rgba(0, 0, 0, 0.2));
    padding: 20px 24px;
    max-width: 400px;
    width: 90vw;
  }

  .confirm-dialog p {
    margin: 0 0 16px;
    font-size: 14px;
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
