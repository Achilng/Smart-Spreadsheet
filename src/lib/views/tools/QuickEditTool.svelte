<script lang="ts">
  import { emitTo } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";

  import {
    applyQuickArtistPrefix,
    applyQuickGroup,
    applyQuickTag,
    createGroup,
    createTag,
    deleteGroup,
    deleteTag,
    getRowsByIds,
    listGroups,
    listTags,
    previewQuickArtistPrefix,
    previewQuickGroup,
    previewQuickTag,
    reapplyQuickArtistPrefixChanges,
    reapplyQuickGroupChanges,
    reapplyQuickTagChanges,
    restoreGroup,
    revertQuickArtistPrefixChanges,
    revertQuickGroupChanges,
    revertQuickTagChanges,
    type GroupSummary,
    type QuickArtistPrefixApplyResult,
    type QuickArtistPrefixPreview,
    type QuickEditCondition,
    type QuickGroupApplyResult,
    type QuickGroupPreview,
    type QuickTagApplyResult,
    type QuickTagPreview,
    type RowRecord,
    type TagSummary,
  } from "../../api";
  import {
    app,
    errorText,
    formatCount,
    notifyMainStateChanged,
    setNotice,
  } from "../../stores/app-state.svelte";
  import {
    history,
    recordHistory,
    redoLastAction,
    undoLastAction,
  } from "../../stores/history.svelte";
  import Thumbnail from "../../ui/Thumbnail.svelte";
  import { focusMainWindow, type ToolboxRowRequest } from "../../windows/toolbox";

  let { active = false }: { active?: boolean } = $props();

  type Operation = "tag" | "group" | "artist";
  type ActivePreview = QuickTagPreview | QuickGroupPreview | QuickArtistPrefixPreview;
  type ActiveResult =
    | QuickTagApplyResult
    | QuickGroupApplyResult
    | QuickArtistPrefixApplyResult;

  let operation = $state<Operation>("tag");
  let promptText = $state("");
  let tagSearch = $state("");
  let newTagName = $state("");
  let tagCreatorOpen = $state(false);
  let newTagInput = $state<HTMLInputElement | undefined>(undefined);
  let tags = $state<TagSummary[]>([]);
  let selectedTags = $state<string[]>([]);
  let groupSearch = $state("");
  let newGroupName = $state("");
  let groupCreatorOpen = $state(false);
  let newGroupInput = $state<HTMLInputElement | undefined>(undefined);
  let groups = $state<GroupSummary[]>([]);
  let selectedGroupId = $state<number | null>(null);
  let onlyUngrouped = $state(false);
  let artistName = $state("");
  let tagsLoading = $state(false);
  let groupsLoading = $state(false);
  let creatingTag = $state(false);
  let creatingGroup = $state(false);
  let previewing = $state(false);
  let applying = $state(false);
  let preview = $state<ActivePreview | null>(null);
  let sampleRows = $state<RowRecord[]>([]);
  let lastResult = $state<ActiveResult | null>(null);
  let error = $state<string | null>(null);
  let openingRowId = $state<number | null>(null);

  const requiredTokens = $derived(parseRequiredTokens(promptText));
  const visibleTags = $derived(
    tagSearch.trim()
      ? tags.filter(tag => tag.name.toLocaleLowerCase().includes(tagSearch.trim().toLocaleLowerCase()))
      : tags,
  );
  const visibleGroups = $derived(
    groupSearch.trim()
      ? groups.filter(group =>
          group.name.toLocaleLowerCase().includes(groupSearch.trim().toLocaleLowerCase())
        )
      : groups,
  );
  const targetReady = $derived(
    operation === "tag"
      ? selectedTags.length > 0
      : operation === "group"
        ? selectedGroupId !== null
        : artistName.trim().length > 0,
  );
  const canPreview = $derived(
    (operation === "artist" || requiredTokens.length > 0) &&
      targetReady &&
      !previewing &&
      !applying &&
      !creatingTag &&
      !creatingGroup &&
      !history.busy &&
      !app.busy,
  );

  onMount(() => {
    void Promise.all([refreshTags(), refreshGroups()]);
  });

  function condition(): QuickEditCondition {
    return {
      fields: ["positivePrompt", "characterPrompt", "negativePrompt", "artists", "note"],
      requiredTokens: [...requiredTokens],
    };
  }

  function parseRequiredTokens(value: string): string[] {
    const seen = new Set<string>();
    const tokens: string[] = [];
    // 与自动规则编辑器一致：半角逗号、全角逗号、换行都是分隔符
    for (const part of value.split(/[,，\n\r]+/)) {
      const token = part.trim();
      const identity = token.toLocaleLowerCase();
      if (token && !seen.has(identity)) {
        seen.add(identity);
        tokens.push(token);
      }
    }
    return tokens;
  }

  function invalidatePreview(): void {
    preview = null;
    sampleRows = [];
    lastResult = null;
    error = null;
  }

  function setOperation(next: Operation): void {
    if (operation === next) return;
    operation = next;
    invalidatePreview();
  }

  function updatePromptText(event: Event): void {
    promptText = (event.currentTarget as HTMLTextAreaElement).value;
    invalidatePreview();
  }

  function updateArtistName(event: Event): void {
    artistName = (event.currentTarget as HTMLInputElement).value;
    invalidatePreview();
  }

  function toggleTag(name: string): void {
    if (selectedTags.includes(name)) {
      selectedTags = selectedTags.filter(tag => tag !== name);
    } else {
      selectedTags = [...selectedTags, name];
    }
    invalidatePreview();
  }

  async function openTagCreator(): Promise<void> {
    tagCreatorOpen = true;
    await tick();
    newTagInput?.focus();
  }

  function closeTagCreator(): void {
    if (creatingTag) return;
    tagCreatorOpen = false;
    newTagName = "";
  }

  function selectGroup(groupId: number): void {
    if (selectedGroupId === groupId) return;
    selectedGroupId = groupId;
    invalidatePreview();
  }

  async function openGroupCreator(): Promise<void> {
    groupCreatorOpen = true;
    await tick();
    newGroupInput?.focus();
  }

  function closeGroupCreator(): void {
    if (creatingGroup) return;
    groupCreatorOpen = false;
    newGroupName = "";
  }

  async function refreshTags(): Promise<void> {
    tagsLoading = true;
    try {
      tags = await listTags();
    } catch (cause) {
      error = `无法读取 Tag 库：${errorText(cause)}`;
    } finally {
      tagsLoading = false;
    }
  }

  async function refreshGroups(): Promise<void> {
    groupsLoading = true;
    try {
      groups = await listGroups();
      if (
        selectedGroupId !== null &&
        !groups.some(group => group.id === selectedGroupId)
      ) {
        selectedGroupId = null;
        invalidatePreview();
      }
    } catch (cause) {
      error = `无法读取分组：${errorText(cause)}`;
    } finally {
      groupsLoading = false;
    }
  }

  async function createNewTag(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const name = newTagName.trim();
    if (!name || creatingTag || history.busy) return;
    creatingTag = true;
    error = null;
    try {
      const created = await createTag(name);
      await refreshTags();
      if (created) {
        await notifyMainStateChanged("libraryEdited");
      }
      if (!selectedTags.includes(name)) {
        selectedTags = [...selectedTags, name];
      }
      newTagName = "";
      tagSearch = "";
      tagCreatorOpen = false;
      invalidatePreview();
      if (created) {
        recordHistory({
          label: `新建 Tag「${name}」`,
          undo: async () => {
            await deleteTag(name);
            selectedTags = selectedTags.filter(tag => tag !== name);
            invalidatePreview();
            await refreshAfterMutation();
          },
          redo: async () => {
            await createTag(name);
            if (!selectedTags.includes(name)) {
              selectedTags = [...selectedTags, name];
            }
            invalidatePreview();
            await refreshAfterMutation();
          },
        });
      }
      setNotice({
        tone: "success",
        text: created ? `已新建并选中 Tag「${name}」。` : `Tag「${name}」已存在，已为你选中。`,
      });
    } catch (cause) {
      error = `新建 Tag 失败：${errorText(cause)}`;
    } finally {
      creatingTag = false;
    }
  }

  async function createNewGroup(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const name = newGroupName.trim();
    if (!name || creatingGroup || history.busy) return;
    creatingGroup = true;
    error = null;
    try {
      const group = await createGroup(name);
      await refreshGroups();
      selectedGroupId = group.id;
      newGroupName = "";
      groupSearch = "";
      groupCreatorOpen = false;
      invalidatePreview();
      recordHistory({
        label: `新建分组「${group.name}」`,
        undo: async () => {
          await deleteGroup(group.id);
          if (selectedGroupId === group.id) {
            selectedGroupId = null;
          }
          invalidatePreview();
          await refreshAfterMutation();
        },
        redo: async () => {
          await restoreGroup(group);
          selectedGroupId = group.id;
          invalidatePreview();
          await refreshAfterMutation();
        },
      });
      await notifyMainStateChanged("libraryEdited");
      setNotice({
        tone: "success",
        text: `已新建并选中分组「${group.name}」。`,
      });
    } catch (cause) {
      error = `新建分组失败：${errorText(cause)}`;
    } finally {
      creatingGroup = false;
    }
  }

  function isTagPreview(value: ActivePreview): value is QuickTagPreview {
    return "associationsToAdd" in value;
  }

  function isArtistPreview(value: ActivePreview): value is QuickArtistPrefixPreview {
    return "promptFieldsNeedingChanges" in value;
  }

  function isGroupPreview(value: ActivePreview): value is QuickGroupPreview {
    return "targetGroupId" in value;
  }

  function isTagResult(value: ActiveResult): value is QuickTagApplyResult {
    return "associationsChanged" in value;
  }

  function isArtistResult(value: ActiveResult): value is QuickArtistPrefixApplyResult {
    return "promptFieldsChanged" in value;
  }

  async function runPreview(): Promise<void> {
    if (!canPreview) return;
    previewing = true;
    error = null;
    lastResult = null;
    try {
      const result = operation === "tag"
        ? await previewQuickTag(condition(), selectedTags)
        : operation === "group"
          ? await previewQuickGroup(condition(), selectedGroupId!, onlyUngrouped)
          : await previewQuickArtistPrefix(artistName.trim());
      const rows = result.sampleRowIds.length > 0
        ? await getRowsByIds(result.sampleRowIds)
        : [];
      preview = result;
      sampleRows = rows;
    } catch (cause) {
      preview = null;
      sampleRows = [];
      error = errorText(cause);
    } finally {
      previewing = false;
    }
  }

  async function runApply(): Promise<void> {
    if (!preview || preview.rowsNeedingChanges === 0 || applying || history.busy || app.busy) return;
    // 全库批量写操作，执行前必须确认；文案写明张数与后果。
    const changeCount = formatCount(preview.rowsNeedingChanges);
    const confirmText =
      operation === "tag"
        ? `将为 ${changeCount} 张图片添加所选 Tag。可撤回。是否执行？`
        : operation === "group"
          ? (preview as QuickGroupPreview).onlyUngrouped
            ? `将把 ${changeCount} 张未分组图片移入目标分组。可撤回。是否执行？`
            : `将把 ${changeCount} 张图片移入目标分组，命中图片原有的分组关系会被替换。可撤回。是否执行？`
          : `将修改 ${changeCount} 张图片的提示词，为「${artistName.trim()}」补全 artist: 前缀。可撤回。是否执行？`;
    if (!window.confirm(confirmText)) return;
    applying = true;
    error = null;
    try {
      if (operation === "tag") {
        const result = await applyQuickTag(condition(), selectedTags);
        lastResult = result;
        if (result.changes.length > 0) {
          const changes = result.changes.map(change => ({ ...change }));
          recordHistory({
            label: `快速打 Tag（${formatCount(result.changedRows)} 张）`,
            undo: async () => {
              await revertQuickTagChanges(changes);
              invalidatePreview();
              await refreshAfterMutation();
            },
            redo: async () => {
              await reapplyQuickTagChanges(changes);
              invalidatePreview();
              await refreshAfterMutation();
            },
          });
        }
        preview = {
          ...(preview as QuickTagPreview),
          rowsNeedingChanges: 0,
          alreadyTaggedRows: preview.matchedRows,
          associationsToAdd: 0,
        };
        setNotice({
          tone: "success",
          text: result.associationsChanged > 0
            ? `快速打标完成：${formatCount(result.changedRows)} 张图片新增了 ${formatCount(result.associationsChanged)} 个 Tag 关联。`
            : "所有命中图片已经拥有所选 Tag，没有产生修改。",
        });
      } else if (operation === "group") {
        const groupId = selectedGroupId!;
        const groupName = groups.find(group => group.id === groupId)?.name ?? "目标分组";
        const groupPreview = preview as QuickGroupPreview;
        const result = await applyQuickGroup(condition(), groupId, groupPreview.onlyUngrouped);
        lastResult = result;
        if (result.changes.length > 0) {
          const changes = result.changes.map(change => ({ ...change }));
          recordHistory({
            label: `${result.onlyUngrouped ? "未分组图片" : "批量"}分组到「${groupName}」（${formatCount(result.changedRows)} 张）`,
            undo: async () => {
              await revertQuickGroupChanges(changes);
              invalidatePreview();
              await refreshAfterMutation();
            },
            redo: async () => {
              await reapplyQuickGroupChanges(changes);
              invalidatePreview();
              await refreshAfterMutation();
            },
          });
        }
        preview = {
          ...groupPreview,
          rowsNeedingChanges: 0,
          alreadyInGroupRows: groupPreview.onlyUngrouped ? 0 : groupPreview.matchedRows,
          skippedGroupedRows: groupPreview.onlyUngrouped ? groupPreview.matchedRows : 0,
        };
        setNotice({
          tone: "success",
          text: result.onlyUngrouped
            ? result.changedRows > 0
              ? `批量分组完成：${formatCount(result.changedRows)} 张未分组图片已加入「${groupName}」，跳过 ${formatCount(result.skippedGroupedRows)} 张已有分组图片。`
              : result.skippedGroupedRows > 0
                ? `命中图片均已有分组，已跳过 ${formatCount(result.skippedGroupedRows)} 张，没有产生修改。`
                : "没有命中可处理的未分组图片。"
            : result.changedRows > 0
              ? `批量分组完成：${formatCount(result.changedRows)} 张图片已分到「${groupName}」。`
              : `所有命中图片已经位于「${groupName}」，没有产生修改。`,
        });
      } else {
        const result = await applyQuickArtistPrefix(artistName.trim());
        lastResult = result;
        if (result.changes.length > 0) {
          const changes = result.changes.map(change => ({ ...change }));
          recordHistory({
            label: `修正画师前缀「${artistName.trim()}」（${formatCount(result.changedRows)} 张）`,
            undo: async () => {
              await revertQuickArtistPrefixChanges(changes);
              invalidatePreview();
              await refreshAfterMutation();
            },
            redo: async () => {
              await reapplyQuickArtistPrefixChanges(changes);
              invalidatePreview();
              await refreshAfterMutation();
            },
          });
        }
        preview = {
          ...(preview as QuickArtistPrefixPreview),
          matchedRows: 0,
          rowsNeedingChanges: 0,
          promptFieldsNeedingChanges: 0,
          sampleRowIds: [],
        };
        sampleRows = [];
        setNotice({
          tone: "success",
          text: result.changedRows > 0
            ? `画师前缀修正完成：${formatCount(result.changedRows)} 张图片的 ${formatCount(result.promptFieldsChanged)} 个提示词字段已更新。`
            : "整个资料库中没有需要修正的对应画师 Tag。",
        });
      }
      await refreshAfterMutation();
    } catch (cause) {
      error = errorText(cause);
    } finally {
      applying = false;
    }
  }

  async function refreshAfterMutation(): Promise<void> {
    await Promise.all([
      refreshTags(),
      refreshGroups(),
      notifyMainStateChanged("libraryEdited"),
    ]);
  }

  async function openInMain(rowId: number): Promise<void> {
    openingRowId = rowId;
    try {
      const request: ToolboxRowRequest = { rowId };
      await emitTo("main", "toolbox://open-row", request);
      await focusMainWindow();
    } catch (cause) {
      setNotice({
        tone: "error",
        text: `无法在主窗口打开图片：${errorText(cause)}`,
      });
    } finally {
      openingRowId = null;
    }
  }

  function rowName(row: RowRecord): string {
    const path = row.imagePath ?? row.storedImagePath;
    return path?.split(/[\\/]/).pop() ?? `图片 #${row.id}`;
  }

  function onKeydown(event: KeyboardEvent): void {
    if (!active || history.busy || applying || previewing) return;
    const target = event.target;
    const isTextEditing =
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      (target instanceof HTMLElement && target.isContentEditable);
    if (!(event.ctrlKey || event.metaKey) || event.altKey || isTextEditing) return;

    const key = event.key.toLocaleLowerCase();
    if (key === "z") {
      event.preventDefault();
      if (event.shiftKey) {
        void redoLastAction();
      } else {
        void undoLastAction();
      }
    } else if (key === "y" && !event.shiftKey) {
      event.preventDefault();
      void redoLastAction();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="quick-edit-page">
  <div class="operation-bar">
    <div class="operation-switcher" aria-label="快速整理操作类型">
      <button
        type="button"
        class:is-active={operation === "tag"}
        onclick={() => setOperation("tag")}
      >添加 Tag</button>
      <button
        type="button"
        class:is-active={operation === "group"}
        onclick={() => setOperation("group")}
      >批量分组</button>
      <button
        type="button"
        class:is-active={operation === "artist"}
        onclick={() => setOperation("artist")}
      >提示词操作</button>
    </div>
    <div class="history-actions">
      <button
        type="button"
        class="btn btn-ghost"
        disabled={history.undoCount === 0 || history.busy || applying || previewing}
        title={history.undoLabel ? `撤回：${history.undoLabel}` : "没有可撤回的快速整理"}
        onclick={() => void undoLastAction()}
      >
        ↶ 撤回
      </button>
      <button
        type="button"
        class="btn btn-ghost"
        disabled={history.redoCount === 0 || history.busy || applying || previewing}
        title={history.redoLabel ? `重做：${history.redoLabel}` : "没有可重做的快速整理"}
        onclick={() => void redoLastAction()}
      >
        ↷ 重做
      </button>
    </div>
  </div>

  <div class="editor-layout">
    <div class="rule-column">
      <section class="rule-card tool-card">
        <div class="step-heading">
          <span class="step-badge">1</span>
          <div>
            <h3>{operation === "artist" ? "输入需要修正的画师名" : "输入提示词组合"}</h3>
            <p>{operation === "artist"
              ? "一次处理一个画师名，不需要填写 artist: 前缀。"
              : "组合中的每一项都必须存在，顺序和位置不限。"}</p>
          </div>
        </div>

        {#if operation === "artist"}
          <input
            class="artist-input"
            type="text"
            value={artistName}
            maxlength="240"
            placeholder="例如：parsley_f"
            aria-label="需要添加 artist 前缀的画师名"
            oninput={updateArtistName}
          />
        {:else}
          <textarea
            value={promptText}
            rows="4"
            placeholder="例如：genshin, hutao（用逗号或换行分隔）"
            aria-label="必须同时存在的提示词组合，支持半角逗号、全角逗号或换行分隔"
            oninput={updatePromptText}
          ></textarea>

          {#if requiredTokens.length > 0}
            <div class="token-list" aria-label="已识别的提示词条件">
              {#each requiredTokens as token (token)}
                <span>{token}</span>
              {/each}
            </div>
          {/if}
        {/if}

        <div class="match-rules">
          <span>扫描范围：整个资料库</span>
          <span>{operation === "artist"
            ? "处理正向、角色与负向提示词；严格匹配完整 Tag 并保留权重格式"
            : "忽略大小写与 NovelAI 权重；girl / 1girl / 1 girl 视为同一项"}</span>
          {#if operation === "artist"}
            <span>已带 artist: 前缀或仅名称相似的 Tag 不会修改</span>
          {/if}
        </div>
      </section>

      {#if operation === "tag"}
        <section class="rule-card tag-card tool-card">
          <div class="step-heading">
            <span class="step-badge">2</span>
            <div>
              <h3>选择要添加的 Tag</h3>
              <p>可以多选；图片原有 Tag 不会被移除。</p>
            </div>
          </div>

          <div class="target-toolbar">
            <input
              class="target-search"
              type="search"
              bind:value={tagSearch}
              placeholder="搜索现有 Tag"
              aria-label="搜索现有 Tag"
            />
            <button
              type="button"
              class="btn btn-ghost"
              aria-expanded={tagCreatorOpen}
              disabled={creatingTag || history.busy}
              onclick={() => tagCreatorOpen ? closeTagCreator() : void openTagCreator()}
            >
              {tagCreatorOpen ? "收起" : "＋ 新建"}
            </button>
          </div>

          {#if tagCreatorOpen}
            <form class="create-target-panel" onsubmit={createNewTag}>
              <input
                bind:this={newTagInput}
                type="text"
                bind:value={newTagName}
                maxlength="120"
                placeholder="输入新 Tag 名称"
                aria-label="新建 Tag 名称"
                disabled={creatingTag}
                onkeydown={event => {
                  if (event.key === "Escape") {
                    event.preventDefault();
                    closeTagCreator();
                  }
                }}
              />
              <div class="create-target-actions">
                <button
                  type="button"
                  class="btn btn-ghost"
                  disabled={creatingTag}
                  onclick={closeTagCreator}
                >取消</button>
                <button
                  type="submit"
                  class="btn"
                  disabled={!newTagName.trim() || creatingTag || history.busy}
                >
                  {creatingTag ? "创建中…" : "创建并选中"}
                </button>
              </div>
            </form>
          {/if}

          <div class="target-list tag-list" aria-label="现有 Tag 列表">
            {#if tagsLoading}
              <p class="list-state">正在读取 Tag 库…</p>
            {:else if tags.length === 0}
              <p class="list-state">Tag 库为空，可以在上方直接新建。</p>
            {:else if visibleTags.length === 0}
              <p class="list-state">没有匹配的 Tag。</p>
            {:else}
              {#each visibleTags as tag (tag.name)}
                <button
                  type="button"
                  class:is-selected={selectedTags.includes(tag.name)}
                  aria-pressed={selectedTags.includes(tag.name)}
                  onclick={() => toggleTag(tag.name)}
                >
                  <span class="check" aria-hidden="true"></span>
                  <strong title={tag.name}>{tag.name}</strong>
                  <small>{formatCount(tag.rowCount)}</small>
                </button>
              {/each}
            {/if}
          </div>

          {#if selectedTags.length > 0}
            <div class="selected-summary">已选择 {formatCount(selectedTags.length)} 个 Tag</div>
          {/if}
        </section>
      {:else if operation === "group"}
        <section class="rule-card group-card tool-card">
          <div class="step-heading">
            <span class="step-badge">2</span>
            <div>
              <h3>选择目标分组</h3>
              <p>命中图片会统一移入这个分组；原分组关系将被替换。</p>
            </div>
          </div>

          <div class="target-toolbar">
            <input
              class="target-search"
              type="search"
              bind:value={groupSearch}
              placeholder="搜索现有分组"
              aria-label="搜索现有分组"
            />
            <button
              type="button"
              class="btn btn-ghost"
              aria-expanded={groupCreatorOpen}
              disabled={creatingGroup || history.busy}
              onclick={() => groupCreatorOpen ? closeGroupCreator() : void openGroupCreator()}
            >
              {groupCreatorOpen ? "收起" : "＋ 新建"}
            </button>
          </div>

          {#if groupCreatorOpen}
            <form class="create-target-panel" onsubmit={createNewGroup}>
              <input
                bind:this={newGroupInput}
                type="text"
                bind:value={newGroupName}
                maxlength="120"
                placeholder="输入新分组名称"
                aria-label="新建分组名称"
                disabled={creatingGroup}
                onkeydown={event => {
                  if (event.key === "Escape") {
                    event.preventDefault();
                    closeGroupCreator();
                  }
                }}
              />
              <div class="create-target-actions">
                <button
                  type="button"
                  class="btn btn-ghost"
                  disabled={creatingGroup}
                  onclick={closeGroupCreator}
                >取消</button>
                <button
                  type="submit"
                  class="btn"
                  disabled={!newGroupName.trim() || creatingGroup || history.busy}
                >
                  {creatingGroup ? "创建中…" : "创建并选中"}
                </button>
              </div>
            </form>
          {/if}

          <div class="target-list group-list" aria-label="现有分组列表">
            {#if groupsLoading}
              <p class="list-state">正在读取分组…</p>
            {:else if groups.length === 0}
              <p class="list-state">还没有分组，可以在上方直接新建。</p>
            {:else if visibleGroups.length === 0}
              <p class="list-state">没有匹配的分组。</p>
            {:else}
              {#each visibleGroups as group (group.id)}
                <button
                  type="button"
                  class:is-selected={selectedGroupId === group.id}
                  aria-pressed={selectedGroupId === group.id}
                  onclick={() => selectGroup(group.id)}
                >
                  <span class="check" aria-hidden="true"></span>
                  <strong title={group.name}>{group.name}</strong>
                  <small>{formatCount(group.memberCount)} 张</small>
                </button>
              {/each}
            {/if}
          </div>

          {#if selectedGroupId !== null}
            <div class="selected-summary">
              目标：{groups.find(group => group.id === selectedGroupId)?.name ?? "已删除的分组"}
            </div>
          {/if}

          <label class="group-scope-option">
            <input
              type="checkbox"
              bind:checked={onlyUngrouped}
              onchange={invalidatePreview}
            />
            <span>
              <strong>仅处理未分组的图片</strong>
              <small>已有任意分组的命中图片会跳过，不会从原分组移出。</small>
            </span>
          </label>
        </section>
      {/if}
    </div>

    <section class="preview-card tool-card">
      <div class="preview-heading">
        <div>
          <h3>执行预览</h3>
          <p>先扫描并确认影响范围，再执行修改。</p>
        </div>
        <button
          type="button"
          class="btn"
          disabled={!canPreview}
          onclick={() => void runPreview()}
        >
          {previewing ? "扫描中…" : "预览匹配结果"}
        </button>
      </div>

      {#if error}
        <p class="error-message">{error}</p>
      {:else if preview}
        <div class="metrics" class:is-artist={isArtistPreview(preview)}>
          {#if isArtistPreview(preview)}
            <div>
              <strong>{formatCount(preview.scannedRows)}</strong>
              <span>扫描图片</span>
            </div>
            <div class:is-highlight={preview.rowsNeedingChanges > 0}>
              <strong>{formatCount(preview.rowsNeedingChanges)}</strong>
              <span>需要修正</span>
            </div>
            <div>
              <strong>{formatCount(preview.promptFieldsNeedingChanges)}</strong>
              <span>涉及提示词字段</span>
            </div>
          {:else}
            <div>
              <strong>{formatCount(preview.scannedRows)}</strong>
              <span>扫描图片</span>
            </div>
            <div>
              <strong>{formatCount(preview.matchedRows)}</strong>
              <span>命中组合</span>
            </div>
            <div class:is-highlight={preview.rowsNeedingChanges > 0}>
              <strong>{formatCount(preview.rowsNeedingChanges)}</strong>
              <span>需要修改</span>
            </div>
            <div>
              <strong>
                {formatCount(
                  isTagPreview(preview)
                    ? preview.alreadyTaggedRows
                    : preview.onlyUngrouped
                      ? preview.skippedGroupedRows
                      : preview.alreadyInGroupRows
                )}
              </strong>
              <span>{isTagPreview(preview)
                ? "已有全部 Tag"
                : preview.onlyUngrouped
                  ? "跳过已分组"
                  : "已在目标分组"}</span>
            </div>
          {/if}
        </div>

        {#if sampleRows.length > 0}
          <div class="sample-heading">
            <strong>命中示例</strong>
            <span>最多展示 12 张，点击可在主窗口定位</span>
          </div>
          <div class="sample-grid">
            {#each sampleRows as row (row.id)}
              <button
                type="button"
                title={rowName(row)}
                disabled={openingRowId !== null}
                onclick={() => void openInMain(row.id)}
              >
                <span class="sample-image">
                  <Thumbnail
                    rowId={row.id}
                    hasImage={Boolean(row.imagePath || row.storedImagePath)}
                    alt={rowName(row)}
                  />
                </span>
                <span>{rowName(row)}</span>
              </button>
            {/each}
          </div>
        {:else}
          <div class="empty-preview">
            <strong>{isArtistPreview(preview)
              ? "没有找到需要修正的画师 Tag"
              : isGroupPreview(preview) &&
                  preview.onlyUngrouped &&
                  preview.skippedGroupedRows > 0
                ? "命中图片均已有分组"
                : "没有图片命中这个提示词组合"}</strong>
            <span>{isArtistPreview(preview)
              ? "已带 artist: 前缀的 Tag 会自动跳过。"
              : isGroupPreview(preview) &&
                  preview.onlyUngrouped &&
                  preview.skippedGroupedRows > 0
                ? "已按“仅处理未分组的图片”全部跳过。"
                : "除已列出的泛用别名外，空格和下划线会被严格区分。"}</span>
          </div>
        {/if}

        <div class="apply-panel">
          <div>
            {#if isArtistPreview(preview)}
              {#if preview.rowsNeedingChanges > 0}
                将修正 {formatCount(preview.rowsNeedingChanges)} 张图片中的
                {formatCount(preview.promptFieldsNeedingChanges)} 个提示词字段
              {:else}
                整个资料库中没有需要修正的对应画师 Tag
              {/if}
            {:else if isTagPreview(preview)}
              {#if preview.associationsToAdd > 0}
                将为 {formatCount(preview.rowsNeedingChanges)} 张图片新增
                {formatCount(preview.associationsToAdd)} 个 Tag 关联
              {:else if preview.matchedRows > 0}
                命中图片已经拥有所选 Tag
              {:else}
                当前规则没有可执行的修改
              {/if}
            {:else}
              {#if preview.onlyUngrouped}
                {#if preview.rowsNeedingChanges > 0}
                  将把 {formatCount(preview.rowsNeedingChanges)} 张未分组图片加入
                  「{preview.targetGroupName}」；跳过
                  {formatCount(preview.skippedGroupedRows)} 张已有分组图片
                {:else if preview.skippedGroupedRows > 0}
                  命中图片均已有分组，将全部跳过
                {:else}
                  当前规则没有可执行的修改
                {/if}
              {:else}
                {#if preview.rowsNeedingChanges > 0}
                  将把 {formatCount(preview.rowsNeedingChanges)} 张图片移入
                  「{preview.targetGroupName}」
                {:else if preview.matchedRows > 0}
                  命中图片已经位于「{preview.targetGroupName}」
                {:else}
                  当前规则没有可执行的修改
                {/if}
              {/if}
            {/if}
          </div>
          <button
            type="button"
            class="btn btn-primary"
            disabled={preview.rowsNeedingChanges === 0 || applying || history.busy}
            onclick={() => void runApply()}
          >
            {applying
              ? "正在应用…"
              : operation === "tag"
                ? "执行打标"
                : operation === "group"
                  ? "执行分组"
                  : "修正前缀"}
          </button>
        </div>

        {#if lastResult}
          <p class="result-message">
            {#if isArtistResult(lastResult)}
              已修正 {formatCount(lastResult.changedRows)} 张图片中的
              {formatCount(lastResult.promptFieldsChanged)} 个提示词字段。
            {:else if isTagResult(lastResult)}
              已修改 {formatCount(lastResult.changedRows)} 张图片，共新增
              {formatCount(lastResult.associationsChanged)} 个 Tag 关联。
            {:else}
              已将 {formatCount(lastResult.changedRows)} 张图片移入目标分组。{#if lastResult.onlyUngrouped}
                跳过 {formatCount(lastResult.skippedGroupedRows)} 张已有分组图片。
              {/if}
            {/if}
          </p>
        {/if}
      {:else}
        <div class="preview-placeholder">
          <span class="preview-icon">⌕</span>
          <strong>等待预览</strong>
          <p>{operation === "tag"
            ? "输入提示词组合并选择目标 Tag 后，扫描整个资料库。"
            : operation === "group"
              ? "输入提示词组合并选择目标分组后，扫描整个资料库。"
              : "输入一个画师名后，扫描整个资料库的三类提示词。"}</p>
        </div>
      {/if}
    </section>
  </div>
</div>

<style>
  .quick-edit-page {
    min-height: 100%;
    padding: 20px 24px 30px;
  }

  .operation-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 14px;
  }

  .operation-switcher {
    display: inline-flex;
    padding: 3px;
    gap: 2px;
    border-radius: var(--radius-full);
    background: var(--surface-3);
  }

  .operation-switcher button {
    min-width: 92px;
    height: 28px;
    padding: 0 16px;
    border: 0;
    border-radius: var(--radius-full);
    background: transparent;
    color: var(--text-2);
    font-size: 12.5px;
  }

  .operation-switcher button:hover:not(.is-active) {
    color: var(--text);
  }

  .operation-switcher button.is-active {
    background: var(--surface);
    color: var(--text);
    font-weight: 600;
    box-shadow: 0 1px 4px rgb(0 0 0 / 10%);
  }

  .history-actions {
    display: flex;
    gap: 2px;
  }

  .history-actions .btn {
    padding-inline: 10px;
    font-size: var(--font-sm);
  }

  .editor-layout {
    display: grid;
    grid-template-columns: minmax(300px, 0.82fr) minmax(340px, 1.18fr);
    gap: 16px;
    align-items: start;
  }

  .rule-column {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
  }

  .rule-card,
  .preview-card {
    min-width: 0;
  }

  .rule-card {
    padding: 18px;
  }

  .step-heading {
    display: flex;
    align-items: flex-start;
    gap: 11px;
    margin-bottom: 14px;
  }


  .step-heading h3,
  .preview-heading h3 {
    font-size: var(--font-lg);
  }

  .step-heading p,
  .preview-heading p {
    margin-top: 2px;
    color: var(--text-3);
    font-size: var(--font-sm);
  }

  textarea,
  .artist-input,
  .target-search,
  .create-target-panel input {
    width: 100%;
  }

  textarea {
    min-height: 88px;
    resize: vertical;
    padding: 10px 11px;
    line-height: 1.55;
  }

  textarea:focus,
  .artist-input:focus,
  .target-search:focus,
  .create-target-panel input:focus {
    border-color: var(--accent);
    box-shadow: var(--focus-ring);
  }

  .token-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
  }

  .artist-input {
    width: 100%;
    height: 39px;
    padding: 0 11px;
  }

  .token-list span {
    max-width: 100%;
    overflow: hidden;
    padding: 3px 8px;
    border-radius: var(--radius-full);
    background: var(--accent-soft);
    color: var(--accent);
    font-size: var(--font-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .match-rules {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin-top: 13px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .tag-card {
    padding-bottom: 13px;
  }

  .target-toolbar {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 7px;
  }

  .target-search {
    height: 35px;
    padding: 0 10px;
  }

  .target-toolbar .btn {
    min-width: 74px;
    padding-inline: 11px;
    font-size: var(--font-sm);
  }

  .create-target-panel {
    display: grid;
    gap: 9px;
    margin-top: 9px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--surface-2);
  }

  .create-target-panel input {
    width: 100%;
    height: 35px;
    padding: 0 10px;
  }

  .create-target-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }

  .create-target-actions .btn {
    padding-inline: 11px;
    font-size: var(--font-sm);
  }

  .target-list {
    max-height: 210px;
    min-height: 72px;
    margin-top: 9px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
  }

  .target-list button {
    width: 100%;
    min-height: 36px;
    display: grid;
    grid-template-columns: 20px minmax(0, 1fr) auto;
    align-items: center;
    gap: 7px;
    padding: 5px 9px;
    border: 0;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    text-align: left;
  }

  .target-list button:last-child {
    border-bottom: 0;
  }

  .target-list button:hover {
    background: var(--surface-2);
  }

  .target-list button.is-selected {
    color: var(--text);
  }

  .target-list button.is-selected strong {
    font-weight: 700;
  }

  .target-list .check {
    width: 16px;
    height: 16px;
    border: 1.5px solid var(--border-strong);
    border-radius: 5px;
    background: var(--surface);
    transition:
      background var(--motion-fast) var(--ease-responsive),
      border-color var(--motion-fast) var(--ease-responsive);
  }

  .target-list button.is-selected .check {
    background-color: var(--primary);
    background-image: url('data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M4 8.5 6.8 11 12 5.5" fill="none" stroke="white" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>');
    background-position: center;
    background-size: 12px;
    background-repeat: no-repeat;
    border-color: var(--primary);
  }

  .target-list strong {
    overflow: hidden;
    font-size: var(--font-sm);
    font-weight: 550;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .target-list small {
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .list-state {
    display: grid;
    min-height: 72px;
    place-items: center;
    padding: 12px;
    color: var(--text-3);
    font-size: var(--font-sm);
    text-align: center;
  }

  .selected-summary {
    margin-top: 7px;
    color: var(--accent);
    font-size: var(--font-xs);
  }

  .group-scope-option {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    margin-top: 12px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--surface-2);
    color: var(--text-2);
    cursor: pointer;
  }

  .group-scope-option input {
    flex: none;
    margin-top: 1px;
  }

  .group-scope-option span,
  .group-scope-option strong,
  .group-scope-option small {
    display: block;
  }

  .group-scope-option strong {
    font-size: var(--font-sm);
  }

  .group-scope-option small {
    margin-top: 2px;
    color: var(--text-3);
    font-size: var(--font-xs);
    line-height: 1.4;
  }

  .preview-card {
    min-height: 480px;
    display: flex;
    flex-direction: column;
    padding: 18px;
  }

  .preview-heading {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
  }

  .preview-heading .btn {
    padding-inline: 12px;
    font-size: var(--font-sm);
  }

  .metrics {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
    margin-top: 17px;
  }

  .metrics.is-artist {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .metrics > div {
    min-width: 0;
    padding: 10px;
    border-radius: var(--radius-s);
    background: var(--surface-2);
  }

  .metrics > div.is-highlight {
    background: var(--accent-soft);
  }

  .metrics strong,
  .metrics span {
    display: block;
  }

  .metrics strong {
    overflow: hidden;
    font-size: var(--font-lg);
    text-overflow: ellipsis;
  }

  .metrics span {
    margin-top: 1px;
    color: var(--text-3);
    font-size: var(--font-xs);
    white-space: nowrap;
  }

  .sample-heading {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
    margin: 17px 1px 8px;
    font-size: var(--font-sm);
  }

  .sample-heading span {
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .sample-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 7px;
  }

  .sample-grid button {
    min-width: 0;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--surface);
    text-align: left;
  }

  .sample-grid button:hover:not(:disabled) {
    border-color: var(--accent);
  }

  .sample-image {
    height: 72px;
    display: block;
    overflow: hidden;
    background: var(--surface-3);
  }

  .sample-grid button > span:last-child {
    display: block;
    overflow: hidden;
    padding: 5px 6px;
    color: var(--text-2);
    font-size: var(--font-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .apply-panel {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    margin-top: auto;
    padding-top: 16px;
    border-top: 1px solid var(--border);
    color: var(--text-2);
    font-size: var(--font-sm);
  }

  .result-message,
  .error-message {
    margin-top: 10px;
    padding: 9px 11px;
    border-radius: var(--radius-s);
    font-size: var(--font-sm);
  }

  .result-message {
    background: var(--success-soft);
    color: var(--success);
  }

  .error-message {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .preview-placeholder,
  .empty-preview {
    flex: 1;
    min-height: 230px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--text-3);
    text-align: center;
  }

  .preview-placeholder strong,
  .empty-preview strong {
    color: var(--text-2);
    font-size: var(--font-md);
  }

  .preview-placeholder p,
  .empty-preview span {
    max-width: 300px;
    margin-top: 3px;
    font-size: var(--font-sm);
  }

  .preview-icon {
    width: 40px;
    height: 40px;
    display: grid;
    place-items: center;
    margin-bottom: 8px;
    border-radius: var(--radius-m);
    background: var(--surface-3);
    color: var(--text-3);
    font-size: 20px;
  }

  @media (max-width: 840px) {
    .quick-edit-page {
      padding: 16px;
    }

    .editor-layout {
      grid-template-columns: 1fr;
    }

    .preview-card {
      min-height: 430px;
    }
  }

  @media (max-width: 680px) {
    .operation-bar {
      align-items: flex-start;
      flex-direction: column;
    }

    .metrics {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
