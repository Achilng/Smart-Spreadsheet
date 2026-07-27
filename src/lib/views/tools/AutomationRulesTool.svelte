<script lang="ts">
  import { emitTo } from "@tauri-apps/api/event";
  import { confirm as confirmDialog, open, save } from "@tauri-apps/plugin-dialog";
  import ArrowDown from "@lucide/svelte/icons/arrow-down";
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import CheckCircle2 from "@lucide/svelte/icons/check-circle-2";
  import ClipboardCopy from "@lucide/svelte/icons/clipboard-copy";
  import ClipboardPaste from "@lucide/svelte/icons/clipboard-paste";
  import FlaskConical from "@lucide/svelte/icons/flask-conical";
  import ImportIcon from "@lucide/svelte/icons/import";
  import Plus from "@lucide/svelte/icons/plus";
  import Save from "@lucide/svelte/icons/save";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Zap from "@lucide/svelte/icons/zap";
  import { onMount } from "svelte";
  import { flip } from "svelte/animate";

  import { flipDuration } from "../../ui/motion";
  import { buildAutomationRuleAiPrompt } from "../../automation-rule-ai-prompt";

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
  } from "../../api";
  import {
    app,
    errorText,
    formatCount,
    notifyMainStateChanged,
    setNotice,
  } from "../../stores/app-state.svelte";
  import { registerCloseGuard } from "../../stores/close-guard";
  import { clearHistory } from "../../stores/history.svelte";
  import Thumbnail from "../../ui/Thumbnail.svelte";
  import Dropdown, { type DropdownItem } from "../../ui/Dropdown.svelte";
  import { focusMainWindow, type ToolboxRowRequest } from "../../windows/toolbox";
  import RuleActionEditor from "./RuleActionEditor.svelte";
  import RuleConditionEditor from "./RuleConditionEditor.svelte";
  import RuleImportDialog from "./RuleImportDialog.svelte";
  import RuleTextImportDialog from "./RuleTextImportDialog.svelte";

  const initialDraft = emptyAutomationRuleDraft();
  type PendingRuleImport =
    | { kind: "file"; path: string }
    | { kind: "text"; text: string };

  let rules = $state<AutomationRule[]>([]);
  let groups = $state<GroupSummary[]>([]);
  let tags = $state<TagSummary[]>([]);
  let tagsLoading = $state(false);
  let selectedId = $state<number | null>(null);
  let draft = $state<AutomationRuleDraft>(plainClone(initialDraft));
  let baseline = $state<AutomationRuleDraft>(plainClone(initialDraft));
  let loading = $state(true);
  let saving = $state(false);
  let testing = $state(false);
  let running = $state(false);
  let deleting = $state(false);
  let error = $state<string | null>(null);
  let preview = $state<RulePreview | null>(null);
  let execution = $state<RuleExecutionSummary | null>(null);
  let sampleRows = $state<RowRecord[]>([]);
  let openingRowId = $state<number | null>(null);
  let transferring = $state(false);
  let copyingPrompt = $state(false);
  let pendingImport = $state<PendingRuleImport | null>(null);
  let importInspection = $state<AutomationRuleImportInspection | null>(null);
  let textImportOpen = $state(false);
  let importText = $state("");
  let textImportError = $state<string | null>(null);

  const selectedRule = $derived(rules.find(rule => rule.id === selectedId) ?? null);
  const dirty = $derived(JSON.stringify(draft) !== JSON.stringify(baseline));
  const conditionCount = $derived(
    draft.conditions.groups.reduce((sum, group) => sum + group.conditions.length, 0),
  );
  const exportItems = $derived.by((): DropdownItem[] => {
    const items: DropdownItem[] = [];
    if (selectedRule) {
      items.push({
        label: "导出当前规则",
        hint: "导出已保存版本",
        action: () => void chooseRuleExport([selectedRule.id], selectedRule.name),
      });
    }
    items.push({
      label: "导出全部规则",
      hint: `${rules.length} 条已保存规则`,
      action: () => void chooseRuleExport(rules.map(rule => rule.id), "自动规则"),
    });
    return items;
  });

  onMount(() => {
    void initialize();
    // 规则草稿有未保存修改时，拦截关窗
    return registerCloseGuard(() => {
      if (transferring) return "规则文件正在导入或导出";
      if ((textImportOpen && importText.trim()) || pendingImport?.kind === "text") {
        return "粘贴的 JSON 文本尚未确认导入";
      }
      return dirty ? `自动规则「${draft.name.trim() || "未命名规则"}」有未保存的修改` : null;
    });
  });

  function plainClone<T>(value: T): T {
    return JSON.parse(JSON.stringify(value)) as T;
  }

  /** 设置错误并把顶部错误横幅滚进视野——长表单里报错点可能在屏幕外。 */
  function showError(message: string): void {
    error = message;
    requestAnimationFrame(() => {
      document.querySelector(".error-banner")?.scrollIntoView({ block: "nearest", behavior: "smooth" });
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

  function closeRuleTextImport(): void {
    if (transferring) return;
    if (importText.trim() && !window.confirm("已粘贴的 JSON 文本尚未导入，确定要放弃吗？")) return;
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

  function closeRuleImport(): void {
    if (transferring) return;
    if (
      pendingImport?.kind === "text" &&
      !window.confirm("已检查的 JSON 文本尚未导入，确定要放弃吗？")
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
    const keepDraft = dirty;
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
    if (dirty) {
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

  function confirmDiscard(): boolean {
    return !dirty || window.confirm("当前规则还有未保存的修改，确定要放弃吗？");
  }

  function loadRule(rule: AutomationRule, checkDirty = false): void {
    if (checkDirty && !confirmDiscard()) return;
    selectedId = rule.id;
    draft = cloneDraft(rule);
    baseline = cloneDraft(rule);
    preview = null;
    execution = null;
    sampleRows = [];
    error = null;
  }

  function startNew(checkDirty = true): void {
    if (checkDirty && !confirmDiscard()) return;
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
    try {
      await setAutomationRuleEnabled(rule.id, enabled);
      rule.enabled = enabled;
      if (selectedId === rule.id) {
        draft.enabled = enabled;
        baseline.enabled = enabled;
      }
    } catch (cause) {
      error = errorText(cause);
    }
  }

  async function moveRule(index: number, offset: number): Promise<void> {
    const target = index + offset;
    if (target < 0 || target >= rules.length) return;
    const ordered = [...rules];
    [ordered[index], ordered[target]] = [ordered[target], ordered[index]];
    try {
      await reorderAutomationRules(ordered.map(rule => rule.id));
      rules = await listAutomationRules();
    } catch (cause) {
      error = errorText(cause);
    }
  }

  async function removeSelectedRule(): Promise<void> {
    if (!selectedRule || deleting) return;
    if (!window.confirm(`确定删除规则「${selectedRule.name}」吗？这不会撤销它过去做过的修改。`)) return;
    deleting = true;
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

  function removeGroup(index: number): void {
    if (draft.conditions.groups.length === 1) {
      showError("规则至少需要一个条件组。");
      return;
    }
    const group = draft.conditions.groups[index];
    if (
      group.conditions.length > 1 &&
      !window.confirm(`条件组 ${index + 1} 里有 ${group.conditions.length} 个条件，删除整组会一并移除，确定吗？`)
    ) {
      return;
    }
    draft.conditions.groups.splice(index, 1);
    resetResult();
  }

  function defaultAction(type: RuleAction["type"]): RuleAction {
    switch (type) {
      case "addTags": return { type, tags: [] };
      case "removeTags": return { type, tags: [] };
      case "setGroup": return { type, groupId: 0, onlyIfUngrouped: false };
      case "clearGroup": return { type };
      case "appendPrompt": return { type, field: "positive", value: "" };
      case "deletePromptTags": return { type, field: "positive", value: "" };
      case "replacePrompt": return { type, field: "positive", find: "", replace: "", caseSensitive: true };
      case "prefixArtist": return { type, artists: [] };
      case "setNote": return { type, value: "" };
      case "appendNote": return { type, value: "", separator: "\n" };
      case "clearNote": return { type };
      case "stopProcessing": return { type };
    }
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
    if (testing || app.busy) return;
    testing = true;
    error = null;
    execution = null;
    try {
      // 草稿（未保存/有未保存修改）直接走只读草稿预览，不再强迫先保存
      preview = selectedId !== null && !dirty
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
    if (selectedId === null || dirty || !preview || running || app.busy || preview.rowsNeedingChanges === 0) return;
    if (!window.confirm(
      `规则将修改现有资料库中 ${formatCount(preview.rowsNeedingChanges)} 张图片（命中 ${formatCount(preview.matchedRows)} 张）。本操作不进入撤销记录，若产生修改会清空当前撤销/重做记录。确定执行吗？`,
    )) return;
    running = true;
    error = null;
    try {
      execution = await runAutomationRuleOnLibrary(selectedId);
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
      const request: ToolboxRowRequest = { rowId };
      await emitTo("main", "toolbox://open-row", request);
      await focusMainWindow();
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
</script>

{#if loading}
  <div class="center-state empty-state"><span class="spinner" aria-hidden="true"></span>正在读取规则…</div>
{:else}
  <div class="rules-layout">
    <aside class="rules-sidebar">
      <div class="sidebar-head">
        <div><strong>规则列表</strong><span>{rules.length} 条</span></div>
        <button type="button" class="btn btn-primary compact" onclick={() => startNew()}><Plus size={15} />新建</button>
      </div>
      <div class="sidebar-transfer">
        <button type="button" class="btn compact" disabled={transferring} onclick={() => void chooseRuleImport()}><ImportIcon size={14} />导入 JSON</button>
        <Dropdown label="导出 JSON" items={exportItems} disabled={transferring || rules.length === 0} />
        <button type="button" class="btn compact wide-transfer" disabled={transferring} onclick={openRuleTextImport}><ClipboardPaste size={14} />粘贴 JSON 文本</button>
        <button type="button" class="btn compact ai-prompt-copy" disabled={transferring || copyingPrompt} onclick={() => void copyAiRulePrompt()}><ClipboardCopy size={14} />{copyingPrompt ? "正在准备…" : "复制 AI 编写提示词"}</button>
      </div>

      {#if rules.length === 0}
        <div class="empty-rules empty-state"><Zap size={24} /><strong>还没有规则</strong><p>新建一条规则后，导入图片时就能自动整理。</p></div>
      {:else}
        <div class="rule-list">
          {#each rules as rule, index (rule.id)}
            <article
              class:is-selected={selectedId === rule.id}
              class:is-disabled={!rule.enabled}
              animate:flip={{ duration: flipDuration(170) }}
            >
              <button type="button" class="rule-main" onclick={() => loadRule(rule, true)}>
                <span class="rule-order tabular" aria-hidden="true">{index + 1}</span>
                <span class="rule-copy">
                  <strong>{rule.name || "未命名规则"}</strong>
                  <span class="rule-sub">{ruleSubtitle(rule)}</span>
                </span>
              </button>
              <div class="rule-side">
                <div class="rule-move">
                  <button type="button" title="上移" disabled={index === 0} onclick={() => void moveRule(index, -1)}><ArrowUp size={13} /></button>
                  <button type="button" title="下移" disabled={index === rules.length - 1} onclick={() => void moveRule(index, 1)}><ArrowDown size={13} /></button>
                </div>
                <input
                  type="checkbox"
                  class="switch"
                  title={rule.enabled ? "已启用，点击停用" : "已停用，点击启用"}
                  aria-label={`${rule.enabled ? "停用" : "启用"}规则「${rule.name}」`}
                  checked={rule.enabled}
                  onchange={event => void toggleRule(rule, (event.currentTarget as HTMLInputElement).checked)}
                />
              </div>
            </article>
          {/each}
        </div>
      {/if}
    </aside>

    <main class="rule-editor">
      <header class="editor-head">
        <div><span class="eyebrow">{selectedId === null ? "新规则" : `规则 #${selectedId}`}</span><h2>{draft.name.trim() || "未命名规则"}</h2></div>
        <div class="editor-actions">
          {#if selectedRule}<button type="button" class="btn danger-ghost" disabled={deleting} onclick={() => void removeSelectedRule()}><Trash2 size={15} />删除</button>{/if}
          <button type="button" class="btn btn-primary" disabled={saving || !dirty} onclick={() => void saveRule()}><Save size={15} />{saving ? "保存中…" : "保存规则"}</button>
        </div>
      </header>

      {#if error}<div class="error-banner" role="alert">{error}</div>{/if}

      <div class="editor-body">
        <section class="editor-section basics tool-card">
          <div class="section-title"><span class="step-badge">1</span><div><h3>名称与触发时机</h3><p>说明这条规则何时参与自动处理。</p></div></div>
          <div class="field-grid">
            <label><span>规则名称</span><input value={draft.name} placeholder="例如：识别某个角色" oninput={event => { draft.name = (event.currentTarget as HTMLInputElement).value; resetResult(); }} /></label>
            <label><span>说明（可选）</span><input value={draft.description} placeholder="记录这条规则的用途" oninput={event => { draft.description = (event.currentTarget as HTMLInputElement).value; resetResult(); }} /></label>
          </div>
          <div class="trigger-row">
            <label><input type="checkbox" checked={draft.enabled} onchange={event => { draft.enabled = (event.currentTarget as HTMLInputElement).checked; resetResult(); }} />启用规则</label>
            <label><input type="checkbox" checked={draft.runOnImport} onchange={event => { draft.runOnImport = (event.currentTarget as HTMLInputElement).checked; resetResult(); }} />新图片导入后自动执行</label>
            <label><input type="checkbox" checked={draft.runOnUpdate} onchange={event => { draft.runOnUpdate = (event.currentTarget as HTMLInputElement).checked; resetResult(); }} />更新现有图片后自动执行</label>
          </div>
        </section>

        <section class="editor-section tool-card">
          <div class="section-title"><span class="step-badge">2</span><div><h3>条件检查</h3><p>每张图片单独判断；条件组可以表达 AND 与 OR。</p></div></div>
          <div class="logic-toolbar">
            <label><span>条件组之间</span><select value={draft.conditions.mode} onchange={event => { draft.conditions.mode = (event.currentTarget as HTMLSelectElement).value as "all" | "any"; resetResult(); }}><option value="any">满足任意一组（OR）</option><option value="all">必须满足全部组（AND）</option></select></label>
            <label><span>执行时机</span><select value={draft.conditions.negate ? "notMatched" : "matched"} onchange={event => { draft.conditions.negate = (event.currentTarget as HTMLSelectElement).value === "notMatched"; resetResult(); }}><option value="matched">条件成立时执行</option><option value="notMatched">条件不成立时执行</option></select></label>
            <span class="logic-summary">共 {draft.conditions.groups.length} 组、{conditionCount} 个条件</span>
          </div>

          <div class="condition-groups">
            {#each draft.conditions.groups as group, groupIndex (`group-${groupIndex}`)}
              <article class="condition-group">
                <header><div><strong>条件组 {groupIndex + 1}</strong><select value={group.mode} onchange={event => { group.mode = (event.currentTarget as HTMLSelectElement).value as "all" | "any"; resetResult(); }}><option value="all">组内条件全部成立（AND）</option><option value="any">组内任意条件成立（OR）</option></select></div><button type="button" class="text-danger" disabled={draft.conditions.groups.length === 1} onclick={() => removeGroup(groupIndex)}>删除组</button></header>
                <div class="condition-list">
                  {#each group.conditions as condition, conditionIndex (`condition-${groupIndex}-${conditionIndex}`)}
                    <RuleConditionEditor {condition} {groups} onreplace={value => replaceCondition(groupIndex, conditionIndex, value)} onremove={() => removeCondition(groupIndex, conditionIndex)} />
                  {/each}
                </div>
                <button type="button" class="add-inline" onclick={() => addCondition(groupIndex)}><Plus size={14} />添加条件</button>
              </article>
            {/each}
          </div>
          <button type="button" class="btn" onclick={addGroup}><Plus size={15} />添加条件组</button>
        </section>

        <section class="editor-section tool-card">
          <div class="section-title"><span class="step-badge">3</span><div><h3>执行任务</h3><p>任务按从上到下的顺序执行；后续规则能看到这些修改。</p></div></div>
          <div class="action-list">
            {#each draft.actions as action, index (`action-${index}`)}
              <RuleActionEditor {action} {groups} {tags} tagsloading={tagsLoading} onrefreshtags={refreshTags} onreplace={value => replaceAction(index, value)} onremove={() => removeAction(index)} onmoveup={() => moveAction(index, -1)} onmovedown={() => moveAction(index, 1)} canmoveup={index > 0} canmovedown={index < draft.actions.length - 1} />
            {/each}
          </div>
          <div class="add-action"><button type="button" class="btn" onclick={addAction}><Plus size={15} />添加任务</button></div>
        </section>

        <section class="editor-section test-section tool-card">
          <div class="section-title"><span class="step-badge">4</span><div><h3>测试与应用</h3><p>测试只读取资料库，未保存的草稿也能直接测试；应用现有图片前会再次确认。</p></div></div>
          {#if selectedId === null || dirty}
            <p class="test-hint">当前是{selectedId === null ? "未保存的新规则" : "有未保存修改的规则"}：可以直接测试查看命中效果；“应用到现有图片”与导入时自动执行需要先保存。</p>
          {/if}
          <div class="test-actions">
            <button type="button" class="btn" disabled={testing || running} onclick={() => void testRule()}><FlaskConical size={15} />{testing ? "测试中…" : "测试现有资料库"}</button>
            <button type="button" class="btn btn-primary" disabled={!preview || selectedId === null || dirty || running || preview.rowsNeedingChanges === 0} onclick={() => void runOnLibrary()}><Zap size={15} />{running ? "执行中…" : "应用到现有图片"}</button>
          </div>

          {#if preview}
            <div class="preview-summary metric-grid tabular"><div><span>扫描</span><strong>{formatCount(preview.scannedRows)}</strong></div><div><span>命中</span><strong>{formatCount(preview.matchedRows)}</strong></div><div><span>需要修改</span><strong>{formatCount(preview.rowsNeedingChanges)}</strong></div><div><span>停止后续</span><strong>{formatCount(preview.stoppedRows)}</strong></div></div>
            {#if preview.matchedRows === 0}<div class="result-empty">没有图片命中当前规则。</div>{/if}
            {#if preview.matchedRows > 0 && preview.rowsNeedingChanges === 0}<div class="result-empty">当前规则不会对现有图片产生可保存的修改，无需手动应用。</div>{/if}
          {/if}

          {#if execution}
            <div class="execution-result"><CheckCircle2 size={18} /><div><strong>执行完成，修改 {formatCount(execution.changedRows)} 张图片</strong>{#each execution.reports as report}<p class:error-line={Boolean(report.error)}>{report.ruleName}：命中 {formatCount(report.matchedRows)}，修改 {formatCount(report.changedRows)}{report.error ? `；失败：${report.error}` : ""}</p>{/each}</div></div>
          {/if}

          {#if sampleRows.length > 0}
            <div class="sample-grid" aria-label="命中示例">
              {#each sampleRows as row (row.id)}
                <button type="button" title="在主窗口查看" disabled={openingRowId === row.id} onclick={() => void openInMain(row.id)}><Thumbnail rowId={row.id} hasImage={Boolean(row.imagePath || row.storedImagePath)} alt={`规则命中图片 ${row.id}`} /><span>#{row.id}</span></button>
              {/each}
            </div>
          {/if}
        </section>
      </div>
    </main>
  </div>

  {#if textImportOpen}
    <RuleTextImportDialog
      value={importText}
      busy={transferring}
      error={textImportError}
      onchange={updateRuleText}
      onclose={closeRuleTextImport}
      oninspect={() => void inspectRuleText()}
    />
  {/if}

  {#if importInspection && pendingImport}
    <RuleImportDialog
      inspection={importInspection}
      sourceName={pendingImport.kind === "file"
        ? pendingImport.path.split(/[\\/]/).pop() ?? pendingImport.path
        : "粘贴的 JSON 文本"}
      busy={transferring}
      onclose={closeRuleImport}
      onconfirm={() => void confirmRuleImport()}
      onback={pendingImport.kind === "text" ? backToRuleText : undefined}
    />
  {/if}
{/if}

<style>
  .center-state { height: 100%; }
  .rules-layout { height: 100%; min-height: 0; display: grid; grid-template-columns: 292px minmax(0, 1fr); }
  .rules-sidebar { min-height: 0; overflow-y: auto; padding: 18px 12px; border-right: 1px solid var(--border); background: var(--surface); }
  .sidebar-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 0 6px 14px; }
  .sidebar-head > div { display: flex; align-items: baseline; gap: 7px; }
  .sidebar-head strong { font-size: var(--font-md); }
  .sidebar-head span { color: var(--text-3); font-size: var(--font-xs); }
  .compact { min-height: 32px; padding: 5px 9px; }
  .sidebar-transfer { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; padding: 0 6px 12px; }
  .sidebar-transfer > button { justify-content: center; }
  .sidebar-transfer > .wide-transfer,
  .sidebar-transfer > .ai-prompt-copy { grid-column: 1 / -1; }
  .sidebar-transfer :global(.dropdown) { min-width: 0; }
  .sidebar-transfer :global(.dropdown > .btn) { width: 100%; min-height: 32px; justify-content: center; padding: 5px 8px; }
  .empty-rules { min-height: 210px; padding: 20px; text-align: center; }
  .empty-rules strong { margin-top: 10px; color: var(--text-2); }
  .empty-rules p { max-width: 210px; margin-top: 5px; font-size: var(--font-sm); line-height: 1.55; }
  .rule-list { display: grid; gap: 2px; }
  .rule-list article { position: relative; display: flex; align-items: center; gap: 6px; padding-right: 9px; border-radius: var(--radius-s); transition: background var(--motion-fast) var(--ease-responsive); }
  .rule-list article:hover { background: var(--surface-2); }
  .rule-list article.is-selected { background: var(--surface-3); }
  .rule-list article.is-selected .rule-copy strong { font-weight: 700; }
  .rule-list article.is-disabled .rule-copy strong { color: var(--text-3); }
  .rule-main { min-width: 0; flex: 1; display: flex; align-items: center; gap: 9px; padding: 9px 0 9px 10px; border: 0; background: transparent; text-align: left; color: var(--text); }
  .rule-order { flex: none; min-width: 14px; color: var(--text-4); font-size: 11px; font-weight: 600; text-align: center; }
  .rule-copy { min-width: 0; display: grid; gap: 2px; }
  .rule-copy strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--font-sm); font-weight: 600; }
  .rule-sub { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-3); font-size: var(--font-xs); }
  .rule-side { flex: none; display: flex; align-items: center; gap: 5px; }
  .rule-move { display: flex; gap: 1px; opacity: 0; transition: opacity var(--motion-fast) var(--ease-responsive); }
  .rule-list article:hover .rule-move,
  .rule-move:focus-within { opacity: 1; }
  .rule-move button { width: 22px; height: 22px; display: grid; place-items: center; border: 0; border-radius: 5px; background: transparent; color: var(--text-3); }
  .rule-move button:hover:not(:disabled) { background: var(--surface-3); color: var(--text); }
  .rule-list article.is-selected .rule-move button:hover:not(:disabled) { background: var(--border-strong); }
  .rule-move button:disabled { opacity: .3; }
  .rule-editor { min-width: 0; min-height: 0; overflow-y: auto; background: var(--bg); }
  .editor-head { position: sticky; top: 0; z-index: 5; min-height: 64px; display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 12px 24px; border-bottom: 1px solid var(--border); background: var(--surface); }
  .eyebrow { color: var(--text-3); font-size: var(--font-xs); font-weight: 650; letter-spacing: var(--ls-caps); text-transform: uppercase; }
  .editor-head h2 { margin-top: 2px; font-size: var(--font-xl); font-weight: 650; }
  .editor-actions { display: flex; gap: 8px; }
  .danger-ghost { color: var(--danger); }
  .error-banner { margin: 16px 24px 0; padding: 10px 12px; border: 1px solid color-mix(in srgb, var(--danger) 35%, var(--border)); border-radius: var(--radius-s); background: var(--danger-soft); color: var(--danger); font-size: var(--font-sm); }
  .editor-body { max-width: 980px; display: grid; gap: 16px; padding: 20px 24px 42px; }
  .editor-section { padding: 18px; }
  .section-title { display: flex; align-items: flex-start; gap: 11px; margin-bottom: 15px; }
  .section-title h3 { font-size: var(--font-lg); }
  .section-title p { margin-top: 2px; color: var(--text-3); font-size: var(--font-sm); }
  .field-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  label { display: grid; gap: 4px; }
  label > span { color: var(--text-3); font-size: var(--font-xs); }
  input:not([type="checkbox"]) { min-height: 32px; padding: 5px 9px; font: inherit; }
  .trigger-row { display: flex; flex-wrap: wrap; gap: 9px 18px; margin-top: 13px; }
  .trigger-row label { display: flex; align-items: center; gap: 7px; color: var(--text-2); font-size: var(--font-sm); }
  .trigger-row input { min-height: 0; }
  .logic-toolbar { display: flex; align-items: end; flex-wrap: wrap; gap: 10px; padding: 11px; border-radius: var(--radius-s); background: var(--surface-2); }
  .logic-summary { min-height: 34px; display: inline-flex; align-items: center; margin-left: auto; color: var(--text-3); font-size: var(--font-sm); }
  .condition-groups { display: grid; gap: 12px; margin: 12px 0; }
  .condition-group { padding: 12px; border: 1px solid var(--border); border-radius: var(--radius-m); background: var(--surface-2); }
  .condition-group > header { display: flex; align-items: center; justify-content: space-between; gap: 10px; margin-bottom: 9px; }
  .condition-group > header > div { display: flex; align-items: center; gap: 10px; }
  .condition-group > header strong { font-size: var(--font-sm); }
  .text-danger { border: 0; background: transparent; padding: 4px 8px; border-radius: var(--radius-full); color: var(--danger); font-size: 12.5px; transition: background var(--motion-fast) var(--ease-responsive); }
  .text-danger:hover:not(:disabled) { background: var(--danger-soft); }
  .text-danger:disabled { opacity: .35; }
  .condition-list, .action-list { display: grid; gap: 9px; }
  .add-inline { display: inline-flex; align-items: center; gap: 5px; margin-top: 9px; padding: 5px 8px; border: 0; border-radius: 6px; background: transparent; color: var(--accent); font-size: var(--font-sm); }
  .add-inline:hover { background: var(--accent-soft); }
  .add-action { display: flex; align-items: center; gap: 8px; margin-top: 11px; }
  .test-actions { display: flex; gap: 8px; }
  .test-hint { margin-bottom: 10px; color: var(--text-3); font-size: var(--font-sm); }
  .preview-summary { margin-top: 14px; }
  .result-empty { margin-top: 10px; color: var(--text-3); font-size: var(--font-sm); }
  .execution-result { display: flex; gap: 9px; margin-top: 12px; padding: 12px; border-radius: var(--radius-s); background: var(--success-soft); color: var(--success); }
  .execution-result p { margin-top: 3px; color: var(--text-2); font-size: var(--font-xs); }
  .execution-result .error-line { color: var(--danger); }
  .sample-grid { margin-top: 12px; }
  .sample-grid :global(.thumbnail-stack) { width: 100%; aspect-ratio: 1; border-radius: 6px; overflow: hidden; }
  @media (max-width: 920px) { .rules-layout { grid-template-columns: 230px minmax(0, 1fr); } .field-grid { grid-template-columns: 1fr; } }
</style>
