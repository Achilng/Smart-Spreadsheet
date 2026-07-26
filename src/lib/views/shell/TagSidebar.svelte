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

  // 侧栏模式提升到 app-state，供底部选择条"打 Tag"联动
  const sidebarMode = $derived(app.sidebarMode);
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
    if (app.sidebarMode !== mode) {
      app.sidebarMode = mode;
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
  let deletingTag = $state(false);

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
    } finally {
      deletingTag = false;
      confirmingDelete = null;
    }
  }
  const switchCount = $derived(
    Number(rowStore.dedupe !== "none") +
      Number(rowStore.singleArtistOnly) +
      Number(rowStore.hasVibe) +
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
      <h3>{sidebarMode === "filter" ? "筛选" : "打标"}</h3>
      {#if sidebarMode === "filter"}
        <p class="header-sub tabular">{filterSummary}</p>
      {/if}
    </div>
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
    <div class="f-group" role="group" aria-label="去重与筛选" in:softFly={{ duration: 145, x: -4, y: 0 }}>
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

    <div class="f-head tag-head" in:softFly={{ duration: 145, x: -4, y: 0 }}>
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
        {@const filterOn = sidebarMode === "filter" && activeTags.includes(entry.name)}
        <button
          type="button"
          class="tag-row check-row"
          class:on={filterOn || (sidebarMode === "tag" && state === "all")}
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
          <span class="cbox" class:is-partial={sidebarMode === "tag" && state === "partial"} aria-hidden="true"></span>
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

  .mode-switch {
    display: flex;
    background: var(--surface-3);
    border-radius: var(--radius-full);
    padding: 3px;
    gap: 2px;
    flex: none;
  }

  .mode-switch button {
    border: none;
    background: transparent;
    border-radius: var(--radius-full);
    padding: 2px 10px;
    font-size: var(--font-xs);
    color: var(--text-2);
    transition:
      background var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive),
      box-shadow var(--motion-fast) var(--ease-responsive),
      transform var(--motion-press) var(--ease-responsive);
  }

  .mode-switch button:active {
    transform: scale(0.96);
  }

  .mode-switch button.is-active {
    background: var(--surface);
    color: var(--text);
    font-weight: 600;
    box-shadow: 0 1px 4px rgb(0 0 0 / 10%);
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
      color var(--motion-fast) var(--ease-responsive);
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

  .cbox.is-partial {
    background-color: var(--primary);
    background-image: linear-gradient(#ffffff, #ffffff);
    background-position: center;
    background-size: 8px 2px;
    background-repeat: no-repeat;
    border-color: var(--primary);
  }

  .tagging-info {
    display: grid;
    gap: 4px;
    padding: 0 16px 10px;
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

  .tag-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag-count,
  .coverage {
    font-size: 11.5px;
    color: var(--text-4);
    font-variant-numeric: tabular-nums;
    flex: none;
  }

  .check-row.on .tag-count,
  .check-row.on .coverage {
    color: var(--text-3);
  }

  .coverage.is-partial {
    color: var(--warning);
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
    font-size: var(--font-sm);
  }

  .create-form input:focus {
    outline: none;
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
</style>
