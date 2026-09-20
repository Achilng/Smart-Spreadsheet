import { requestConfirmation } from "../../ui/ConfirmationDialog";
import { errorText, formatCount } from "../../../lib/utils/format";
import { notifyMainStateChanged } from "../../../lib/windows/library-events";
import { notify } from "../notices";
import { runTask, useTasks } from "../tasks";
import { defaultAction } from "../../../lib/features/automation/rule-defaults";
import { confirm as confirmDialog, open, save } from "@tauri-apps/plugin-dialog";
import { useEffect, useState, useSyncExternalStore } from "react";
import { buildAutomationRuleAiPrompt } from "../../../lib/automation-rule-ai-prompt";
import {
  createAutomationRule,
  deleteAutomationRule,
  emptyAutomationRuleDraft,
  exportAutomationRules,
  getRowsByIds,
  importAutomationRuleFile,
  importAutomationRuleText,
  inspectAutomationRuleFile,
  inspectAutomationRuleText,
  listAutomationRules,
  listGroups,
  listTags,
  previewAutomationRule,
  previewAutomationRuleDraft,
  reorderAutomationRules,
  runAutomationRuleOnLibrary,
  setAutomationRuleEnabled,
  updateAutomationRule,
  type AutomationRule,
  type AutomationRuleDraft,
  type AutomationRuleImportInspection,
  type GroupSummary,
  type RowRecord,
  type RuleAction,
  type RuleCondition,
  type RuleExecutionSummary,
  type RulePreview,
  type TagSummary,
} from "../../../lib/api";
import { registerCloseGuard } from "../../../lib/stores/close-guard";
import { clearHistory } from "../history";
import { openRowInMainWindow } from "../../../lib/windows/toolbox";

const setNotice = ({ text, tone }: { text: string; tone: "success" | "error" }) => notify(text, tone);

/** Per-instance state and operations for the AutomationRulesTool view. */
export function createAutomationRulesController(onChange: () => void = () => {}) {

  const initialDraft = emptyAutomationRuleDraft();

  type PendingRuleImport =
    | { kind: "file"; path: string }
    | { kind: "text"; text: string };
  let rules = ([] as AutomationRule[]);
  let groups = ([] as GroupSummary[]);
  let tags = ([] as TagSummary[]);
  let tagsLoading = (false);
  let selectedId = (null as number | null);
  let draft = (plainClone(initialDraft) as AutomationRuleDraft);
  let baseline = (plainClone(initialDraft) as AutomationRuleDraft);
  let loading = (true);
  let saving = (false);
  let testing = (false);
  let running = (false);
  let deleting = (false);
  let reordering = false;
  let error = (null as string | null);
  let preview = (null as RulePreview | null);
  let execution = (null as RuleExecutionSummary | null);
  let sampleRows = ([] as RowRecord[]);
  let openingRowId = (null as number | null);
  let transferring = (false);
  let copyingPrompt = (false);
  let pendingImport = (null as PendingRuleImport | null);
  let importInspection = (null as AutomationRuleImportInspection | null);
  let textImportOpen = (false);
  let importText = ("");
  let textImportError = (null as string | null);
  const getSelectedRule = () => rules.find(rule => rule.id === selectedId) ?? null;
  const getDirty = () => JSON.stringify(draft) !== JSON.stringify(baseline);
  const getConditionCount = () => draft.conditions.groups.reduce((sum, group) => sum + group.conditions.length, 0);
  function closeGuard(): string | null {
    if (saving || running || deleting || transferring || reordering) return "自动规则任务正在处理";
    if ((textImportOpen && importText.trim()) || pendingImport?.kind === "text") return "粘贴的 JSON 文本尚未确认导入";
    return getDirty() ? `自动规则「${draft.name.trim() || "未命名规则"}」有未保存的修改` : null;
  }

  function plainClone<T>(value: T): T {
    return JSON.parse(JSON.stringify(value)) as T;
  }

  /** 设置错误并把顶部错误横幅滚进视野——长表单里报错点可能在屏幕外。 */
  function showError(message: string): void {
    error = message;
    requestAnimationFrame(() => {
      document.querySelector(".automation-rules .error-banner")?.scrollIntoView({ block: "nearest", behavior: "smooth" });
    });
  }

  async function initialize(): Promise<void> {
    loading = true;
    error = null;
    try {
      [rules, groups, tags] = await Promise.all([listAutomationRules(), listGroups(), listTags()]);
      if (rules.length > 0) loadRule(rules[0]);
      else startNew(false);
    } catch (cause) {
      error = errorText(cause);
    } finally {
      loading = false;
    }
  }

  async function refreshTags(): Promise<void> {
    if (tagsLoading) return;
    tagsLoading = true;
    try {
      tags = await listTags();
    } catch (cause) {
      showError(`无法刷新 Tag 列表：${errorText(cause)}`);
    } finally {
      tagsLoading = false;
    }
  }

  async function copyAiRulePrompt(): Promise<void> {
    if (copyingPrompt || transferring) return;
    copyingPrompt = true;
    try {
      const [latestTags, latestGroups] = await Promise.all([listTags(), listGroups()]);
      tags = latestTags;
      groups = latestGroups;
      const prompt = buildAutomationRuleAiPrompt(
        latestTags.map(tag => tag.name),
        latestGroups.map(group => group.name),
      );
      await navigator.clipboard.writeText(prompt);
      setNotice({
        tone: "success",
        text: `AI 编写提示词已复制，包含 ${latestTags.length} 个 Tag 和 ${latestGroups.length} 个分组。`,
      });
    } catch (cause) {
      setNotice({ tone: "error", text: `复制 AI 编写提示词失败：${errorText(cause)}` });
    } finally {
      copyingPrompt = false;
    }
  }

  async function chooseRuleImport(): Promise<void> {
    if (transferring) return;
    const selection = await open({
      multiple: false,
      directory: false,
      title: "选择智能表格规则 JSON",
      filters: [{ name: "JSON 规则文件", extensions: ["json"] }],
    });
    if (typeof selection !== "string") return;
    transferring = true;
    error = null;
    try {
      const inspection = await inspectAutomationRuleFile(selection);
      pendingImport = { kind: "file", path: selection };
      importInspection = inspection;
    } catch (cause) {
      showError(errorText(cause));
      pendingImport = null;
      importInspection = null;
    } finally {
      transferring = false;
    }
  }

  function openRuleTextImport(): void {
    if (transferring) return;
    importText = "";
    textImportError = null;
    textImportOpen = true;
  }

  function updateRuleText(value: string): void {
    importText = value;
    textImportError = null;
  }

  async function closeRuleTextImport(): Promise<void> {
    if (transferring) return;
    if (importText.trim() && !await requestConfirmation("已粘贴的 JSON 文本尚未导入，确定要放弃吗？")) return;
    textImportOpen = false;
    importText = "";
    textImportError = null;
  }

  async function inspectRuleText(): Promise<void> {
    if (transferring || !importText.trim()) return;
    transferring = true;
    textImportError = null;
    try {
      const inspection = await inspectAutomationRuleText(importText);
      pendingImport = { kind: "text", text: importText };
      importInspection = inspection;
      textImportOpen = false;
    } catch (cause) {
      textImportError = errorText(cause);
    } finally {
      transferring = false;
    }
  }

  function backToRuleText(): void {
    if (transferring || pendingImport?.kind !== "text") return;
    importText = pendingImport.text;
    pendingImport = null;
    importInspection = null;
    textImportError = null;
    textImportOpen = true;
  }

  async function closeRuleImport(): Promise<void> {
    if (transferring) return;
    if (
      pendingImport?.kind === "text" &&
      !await requestConfirmation("已检查的 JSON 文本尚未导入，确定要放弃吗？")
    ) return;
    pendingImport = null;
    importInspection = null;
    importText = "";
    textImportError = null;
  }

  async function confirmRuleImport(): Promise<void> {
    if (!pendingImport || !importInspection || transferring) return;
    const source = pendingImport;
    transferring = true;
    error = null;
    const keepDraft = getDirty();
    try {
      const result = source.kind === "file"
        ? await importAutomationRuleFile(source.path, importInspection.contentHash)
        : await importAutomationRuleText(source.text, importInspection.contentHash);
      [rules, groups, tags] = await Promise.all([listAutomationRules(), listGroups(), listTags()]);
      if (!keepDraft) {
        const imported = rules.find(rule => rule.id === result.importedRuleIds[0]);
        if (imported) loadRule(imported);
      }
      if (result.createdTags > 0 || result.createdGroups > 0) {
        clearHistory();
        await notifyMainStateChanged("libraryEdited");
      }
      const details = [
        result.createdTags > 0 && `新建 ${result.createdTags} 个 Tag`,
        result.createdGroups > 0 && `新建 ${result.createdGroups} 个分组`,
        result.renamedRules > 0 && `${result.renamedRules} 条重名规则已改名`,
      ].filter(Boolean);
      setNotice({
        tone: "success",
        text: `已导入 ${result.importedRules} 条规则并保持停用${details.length > 0 ? `；${details.join("，")}` : ""}。`,
      });
      pendingImport = null;
      importInspection = null;
      importText = "";
      textImportError = null;
    } catch (cause) {
      const message = errorText(cause);
      pendingImport = null;
      importInspection = null;
      if (source.kind === "text") {
        importText = source.text;
        textImportError = message;
        textImportOpen = true;
      } else {
        showError(message);
      }
    } finally {
      transferring = false;
    }
  }

  async function chooseRuleExport(ids: number[], name: string): Promise<void> {
    if (transferring || ids.length === 0) return;
    if (getDirty()) {
      const confirmed = await confirmDialog(
        "当前编辑内容尚未保存。导出文件只会包含已保存版本，要继续吗？",
        {
          title: "存在未保存修改",
          kind: "warning",
          okLabel: "导出已保存版本",
          cancelLabel: "取消",
        },
      );
      if (!confirmed) return;
    }
    const destination = await save({
      title: "导出自动规则 JSON（已有文件会被替换）",
      defaultPath: `${safeFileName(name)}.json`,
      filters: [{ name: "JSON 规则文件", extensions: ["json"] }],
    });
    if (typeof destination !== "string") return;
    const outputPath = destination.toLowerCase().endsWith(".json")
      ? destination
      : `${destination}.json`;
    transferring = true;
    error = null;
    try {
      const result = await exportAutomationRules(outputPath, ids);
      setNotice({
        tone: "success",
        text: `已导出 ${result.exportedRules} 条规则到 ${result.path}`,
      });
    } catch (cause) {
      showError(errorText(cause));
    } finally {
      transferring = false;
    }
  }

  function safeFileName(value: string): string {
    const sanitized = value.trim().replace(/[\\/:*?"<>|]/g, "_").replace(/[. ]+$/g, "");
    return sanitized.slice(0, 60) || "自动规则";
  }

  function cloneDraft(rule: AutomationRule | AutomationRuleDraft): AutomationRuleDraft {
    return plainClone({
      name: rule.name,
      description: rule.description,
      enabled: rule.enabled,
      runOnImport: rule.runOnImport,
      runOnUpdate: rule.runOnUpdate,
      conditions: rule.conditions,
      actions: rule.actions,
    });
  }

  async function confirmDiscard(): Promise<boolean> {
    return !getDirty() || await requestConfirmation("当前规则还有未保存的修改，确定要放弃吗？");
  }

  async function loadRule(rule: AutomationRule, checkDirty = false): Promise<void> {
    if (checkDirty && !await confirmDiscard()) return;
    selectedId = rule.id;
    draft = cloneDraft(rule);
    baseline = cloneDraft(rule);
    preview = null;
    execution = null;
    sampleRows = [];
    error = null;
  }

  async function startNew(checkDirty = true): Promise<void> {
    if (checkDirty && !await confirmDiscard()) return;
    const fresh = emptyAutomationRuleDraft();
    selectedId = null;
    draft = plainClone(fresh);
    baseline = plainClone(fresh);
    preview = null;
    execution = null;
    sampleRows = [];
    error = null;
  }

  async function saveRule(): Promise<void> {
    if (saving) return;
    saving = true;
    error = null;
    try {
      const saved = selectedId === null
        ? await createAutomationRule(plainClone(draft))
        : await updateAutomationRule(selectedId, plainClone(draft));
      rules = await listAutomationRules();
      const current = rules.find(rule => rule.id === saved.id) ?? saved;
      loadRule(current);
      setNotice({ tone: "success", text: `规则「${current.name}」已保存。` });
    } catch (cause) {
      showError(errorText(cause));
    } finally {
      saving = false;
    }
  }

  async function toggleRule(rule: AutomationRule, enabled: boolean): Promise<void> {
    if (reordering) return;
    reordering = true;
    try {
      await setAutomationRuleEnabled(rule.id, enabled);
      rule.enabled = enabled;
      if (selectedId === rule.id) {
        draft.enabled = enabled;
        baseline.enabled = enabled;
      }
      resetResult();
    } catch (cause) {
      error = errorText(cause);
    } finally { reordering = false; }
  }

  async function moveRule(index: number, offset: number): Promise<void> {
    if (reordering) return;
    const target = index + offset;
    if (target < 0 || target >= rules.length) return;
    reordering = true;
    const ordered = [...rules];
    [ordered[index], ordered[target]] = [ordered[target], ordered[index]];
    try {
      await reorderAutomationRules(ordered.map(rule => rule.id));
      rules = await listAutomationRules();
    } catch (cause) {
      error = errorText(cause);
    } finally { reordering = false; }
  }

  async function removeSelectedRule(): Promise<void> {
    const selectedRule = getSelectedRule();
    if (!selectedRule || deleting) return;
    if (!await requestConfirmation(`确定删除规则「${selectedRule.name}」吗？这不会撤销它过去做过的修改。`)) return;
    deleting = true; onChange();
    try {
      await deleteAutomationRule(selectedRule.id);
      rules = await listAutomationRules();
      if (rules.length > 0) loadRule(rules[0]);
      else startNew(false);
    } catch (cause) {
      error = errorText(cause);
    } finally {
      deleting = false;
    }
  }

  function resetResult(): void {
    preview = null;
    execution = null;
    sampleRows = [];
  }

  function addCondition(groupIndex: number): void {
    draft.conditions.groups[groupIndex].conditions.push({
      type: "prompt",
      scope: "positiveAndCharacter",
      operator: "containsAll",
      value: "",
      caseSensitive: false,
    });
    resetResult();
  }

  function replaceCondition(groupIndex: number, conditionIndex: number, condition: RuleCondition): void {
    draft.conditions.groups[groupIndex].conditions[conditionIndex] = condition;
    resetResult();
  }

  function removeCondition(groupIndex: number, conditionIndex: number): void {
    const conditions = draft.conditions.groups[groupIndex].conditions;
    if (conditions.length === 1) {
      showError("每个条件组至少需要一个条件；如不需要整组，请删除条件组。");
      return;
    }
    conditions.splice(conditionIndex, 1);
    resetResult();
  }

  function addGroup(): void {
    draft.conditions.groups.push({
      mode: "all",
      conditions: [{ type: "prompt", scope: "positiveAndCharacter", operator: "containsAll", value: "", caseSensitive: false }],
    });
    resetResult();
  }

  async function removeGroup(index: number): Promise<void> {
    if (draft.conditions.groups.length === 1) {
      showError("规则至少需要一个条件组。");
      return;
    }
    const group = draft.conditions.groups[index];
    if (
      group.conditions.length > 1 &&
      !await requestConfirmation(`条件组 ${index + 1} 里有 ${group.conditions.length} 个条件，删除整组会一并移除，确定吗？`)
    ) {
      return;
    }
    draft.conditions.groups.splice(index, 1);
    resetResult();
  }

  function addAction(): void {
    draft.actions.push(defaultAction("addTags"));
    resetResult();
  }

  function replaceAction(index: number, action: RuleAction): void {
    draft.actions[index] = action;
    resetResult();
  }

  function removeAction(index: number): void {
    if (draft.actions.length === 1) {
      showError("规则至少需要一个执行任务。");
      return;
    }
    draft.actions.splice(index, 1);
    resetResult();
  }

  function moveAction(index: number, offset: number): void {
    const target = index + offset;
    if (target < 0 || target >= draft.actions.length) return;
    [draft.actions[index], draft.actions[target]] = [draft.actions[target], draft.actions[index]];
    resetResult();
  }

  async function testRule(): Promise<void> {
    if (testing || useTasks.getState().busy) return;
    testing = true;
    error = null;
    execution = null;
    try {
      // 草稿（未保存/有未保存修改）直接走只读草稿预览，不再强迫先保存
      preview = selectedId !== null && !getDirty()
        ? await previewAutomationRule(selectedId)
        : await previewAutomationRuleDraft(plainClone(draft));
      sampleRows = preview.sampleRowIds.length > 0
        ? await getRowsByIds(preview.sampleRowIds)
        : [];
    } catch (cause) {
      error = errorText(cause);
    } finally {
      testing = false;
    }
  }

  async function runOnLibrary(): Promise<void> {
    if (selectedId === null || getDirty() || !preview || running || useTasks.getState().busy || preview.rowsNeedingChanges === 0) return;
    if (!await requestConfirmation(
      `规则将修改现有资料库中 ${formatCount(preview.rowsNeedingChanges)} 张图片（命中 ${formatCount(preview.matchedRows)} 张）。本操作不进入撤销记录，若产生修改会清空当前撤销/重做记录。确定执行吗？`,
    )) return;
    running = true; onChange();
    error = null;
    try {
      execution = await runTask("应用自动规则", () => runAutomationRuleOnLibrary(selectedId!));
      if (execution.changedRows > 0) clearHistory();
      await notifyMainStateChanged("libraryEdited");
      preview = await previewAutomationRule(selectedId);
      sampleRows = preview.sampleRowIds.length > 0
        ? await getRowsByIds(preview.sampleRowIds)
        : [];
      const failed = Boolean(execution.engineError) || execution.reports.some(report => report.error);
      setNotice({
        tone: failed ? "error" : "success",
        text: failed
          ? `规则执行完成，但存在失败项：已修改 ${formatCount(execution.changedRows)} 张图片，详情见执行结果。`
          : `规则执行完成：修改 ${formatCount(execution.changedRows)} 张图片。`,
      });
    } catch (cause) {
      error = errorText(cause);
    } finally {
      running = false;
    }
  }

  async function openInMain(rowId: number): Promise<void> {
    openingRowId = rowId;
    try {
      await openRowInMainWindow(rowId);
    } catch (cause) {
      setNotice({ tone: "error", text: `无法在主窗口打开图片：${errorText(cause)}` });
    } finally {
      openingRowId = null;
    }
  }

  function ruleSubtitle(rule: AutomationRule): string {
    const triggers = [rule.runOnImport && "导入", rule.runOnUpdate && "更新"].filter(Boolean).join("＋");
    const conditions = rule.conditions.groups.reduce((sum, group) => sum + group.conditions.length, 0);
    return `${triggers || "仅手动"} · ${conditions} 个条件 · ${rule.actions.length} 个任务`;
  }
  return {
    initialize, closeGuard, chooseRuleExport,
    get reordering() { return reordering; },
    get rules() { return rules; },
    set rules(value: typeof rules) { rules = value; },
    get groups() { return groups; },
    set groups(value: typeof groups) { groups = value; },
    get tags() { return tags; },
    set tags(value: typeof tags) { tags = value; },
    get tagsLoading() { return tagsLoading; },
    set tagsLoading(value: typeof tagsLoading) { tagsLoading = value; },
    get selectedId() { return selectedId; },
    set selectedId(value: typeof selectedId) { selectedId = value; },
    get draft() { return draft; },
    set draft(value: typeof draft) { draft = value; },
    get loading() { return loading; },
    set loading(value: typeof loading) { loading = value; },
    get saving() { return saving; },
    set saving(value: typeof saving) { saving = value; },
    get testing() { return testing; },
    set testing(value: typeof testing) { testing = value; },
    get running() { return running; },
    set running(value: typeof running) { running = value; },
    get deleting() { return deleting; },
    set deleting(value: typeof deleting) { deleting = value; },
    get error() { return error; },
    set error(value: typeof error) { error = value; },
    get preview() { return preview; },
    set preview(value: typeof preview) { preview = value; },
    get execution() { return execution; },
    set execution(value: typeof execution) { execution = value; },
    get sampleRows() { return sampleRows; },
    set sampleRows(value: typeof sampleRows) { sampleRows = value; },
    get openingRowId() { return openingRowId; },
    set openingRowId(value: typeof openingRowId) { openingRowId = value; },
    get transferring() { return transferring; },
    set transferring(value: typeof transferring) { transferring = value; },
    get copyingPrompt() { return copyingPrompt; },
    set copyingPrompt(value: typeof copyingPrompt) { copyingPrompt = value; },
    get pendingImport() { return pendingImport; },
    set pendingImport(value: typeof pendingImport) { pendingImport = value; },
    get importInspection() { return importInspection; },
    set importInspection(value: typeof importInspection) { importInspection = value; },
    get textImportOpen() { return textImportOpen; },
    set textImportOpen(value: typeof textImportOpen) { textImportOpen = value; },
    get importText() { return importText; },
    set importText(value: typeof importText) { importText = value; },
    get textImportError() { return textImportError; },
    set textImportError(value: typeof textImportError) { textImportError = value; },
    get selectedRule() { return getSelectedRule(); },
    get dirty() { return getDirty(); },
    get conditionCount() { return getConditionCount(); },
    refreshTags,
    copyAiRulePrompt,
    chooseRuleImport,
    openRuleTextImport,
    updateRuleText,
    closeRuleTextImport,
    inspectRuleText,
    backToRuleText,
    closeRuleImport,
    confirmRuleImport,
    loadRule,
    startNew,
    saveRule,
    toggleRule,
    moveRule,
    removeSelectedRule,
    resetResult,
    addCondition,
    replaceCondition,
    removeCondition,
    addGroup,
    removeGroup,
    addAction,
    replaceAction,
    removeAction,
    moveAction,
    testRule,
    runOnLibrary,
    openInMain,
    ruleSubtitle,
  };
}


/** Each mounted tool owns its draft; React subscribes to completed controller transitions. */
export function useAutomationRules() {
  const [store] = useState(() => {
    let revision = 0;
    const listeners = new Set<() => void>();
    const emit = () => { revision += 1; listeners.forEach(listener => listener()); };
    const source = createAutomationRulesController(emit);
    let initialization: Promise<void> | null = null;
    const controller = new Proxy(source, {
      get(target, key, receiver) {
        const value: unknown = Reflect.get(target, key, receiver);
        if (typeof value !== "function" || key === "closeGuard" || key === "ruleSubtitle") return value;
        return (...args: unknown[]) => {
          try {
            const result: unknown = value(...args);
            emit();
            if (result instanceof Promise) return result.catch(error => notify(errorText(error), "error")).finally(emit);
            return result;
          } catch (error) { notify(errorText(error), "error"); emit(); }
        };
      },
      set(target, key, value) { Reflect.set(target, key, value); emit(); return true; },
    });
    return {
      controller,
      subscribe: (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; },
      snapshot: () => revision,
      initialize: () => { initialization ??= controller.initialize(); return initialization; },
    };
  });
  useSyncExternalStore(store.subscribe, store.snapshot, store.snapshot);
  useEffect(() => {
    void store.initialize();
    return registerCloseGuard(() => store.controller.closeGuard());
  }, [store]);
  return store.controller;
}
