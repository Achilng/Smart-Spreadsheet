import { errorText, formatCount } from "../../utils/format";
import { notifyMainStateChanged } from "../../windows/library-events";
import { setNotice } from "../../stores/notices.svelte";
import { taskState } from "../../stores/task-state.svelte";
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
import { history, recordHistory } from "../../stores/history.svelte";
import { openRowInMainWindow } from "../../windows/toolbox";

/** Per-instance state and operations for the QuickEditTool view. */
export function createQuickEditController() {

  // 撤销快捷键已上移到 ToolboxWindow 窗口级；本面板不再需要 active prop
  // （父组件仍传入 active，多余 prop 会被忽略）。

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
      !taskState.busy,
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
    if (!preview || preview.rowsNeedingChanges === 0 || applying || history.busy || taskState.busy) return;
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
      await openRowInMainWindow(rowId);
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
  return {
    get operation() { return operation; },
    set operation(value: typeof operation) { operation = value; },
    get promptText() { return promptText; },
    set promptText(value: typeof promptText) { promptText = value; },
    get tagSearch() { return tagSearch; },
    set tagSearch(value: typeof tagSearch) { tagSearch = value; },
    get newTagName() { return newTagName; },
    set newTagName(value: typeof newTagName) { newTagName = value; },
    get tagCreatorOpen() { return tagCreatorOpen; },
    set tagCreatorOpen(value: typeof tagCreatorOpen) { tagCreatorOpen = value; },
    get newTagInput() { return newTagInput; },
    set newTagInput(value: typeof newTagInput) { newTagInput = value; },
    get tags() { return tags; },
    set tags(value: typeof tags) { tags = value; },
    get selectedTags() { return selectedTags; },
    set selectedTags(value: typeof selectedTags) { selectedTags = value; },
    get groupSearch() { return groupSearch; },
    set groupSearch(value: typeof groupSearch) { groupSearch = value; },
    get newGroupName() { return newGroupName; },
    set newGroupName(value: typeof newGroupName) { newGroupName = value; },
    get groupCreatorOpen() { return groupCreatorOpen; },
    set groupCreatorOpen(value: typeof groupCreatorOpen) { groupCreatorOpen = value; },
    get newGroupInput() { return newGroupInput; },
    set newGroupInput(value: typeof newGroupInput) { newGroupInput = value; },
    get groups() { return groups; },
    set groups(value: typeof groups) { groups = value; },
    get selectedGroupId() { return selectedGroupId; },
    set selectedGroupId(value: typeof selectedGroupId) { selectedGroupId = value; },
    get onlyUngrouped() { return onlyUngrouped; },
    set onlyUngrouped(value: typeof onlyUngrouped) { onlyUngrouped = value; },
    get artistName() { return artistName; },
    set artistName(value: typeof artistName) { artistName = value; },
    get tagsLoading() { return tagsLoading; },
    set tagsLoading(value: typeof tagsLoading) { tagsLoading = value; },
    get groupsLoading() { return groupsLoading; },
    set groupsLoading(value: typeof groupsLoading) { groupsLoading = value; },
    get creatingTag() { return creatingTag; },
    set creatingTag(value: typeof creatingTag) { creatingTag = value; },
    get creatingGroup() { return creatingGroup; },
    set creatingGroup(value: typeof creatingGroup) { creatingGroup = value; },
    get previewing() { return previewing; },
    set previewing(value: typeof previewing) { previewing = value; },
    get applying() { return applying; },
    set applying(value: typeof applying) { applying = value; },
    get preview() { return preview; },
    set preview(value: typeof preview) { preview = value; },
    get sampleRows() { return sampleRows; },
    set sampleRows(value: typeof sampleRows) { sampleRows = value; },
    get lastResult() { return lastResult; },
    set lastResult(value: typeof lastResult) { lastResult = value; },
    get error() { return error; },
    set error(value: typeof error) { error = value; },
    get openingRowId() { return openingRowId; },
    set openingRowId(value: typeof openingRowId) { openingRowId = value; },
    get requiredTokens() { return requiredTokens; },
    get visibleTags() { return visibleTags; },
    get visibleGroups() { return visibleGroups; },
    get canPreview() { return canPreview; },
    invalidatePreview,
    setOperation,
    updatePromptText,
    updateArtistName,
    toggleTag,
    openTagCreator,
    closeTagCreator,
    selectGroup,
    openGroupCreator,
    closeGroupCreator,
    createNewTag,
    createNewGroup,
    isTagPreview,
    isArtistPreview,
    isGroupPreview,
    isTagResult,
    isArtistResult,
    runPreview,
    runApply,
    openInMain,
    rowName,
  };
}
