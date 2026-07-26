<script lang="ts">
  import { emitTo } from "@tauri-apps/api/event";
  import ArrowDown from "@lucide/svelte/icons/arrow-down";
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import CheckCircle2 from "@lucide/svelte/icons/check-circle-2";
  import FlaskConical from "@lucide/svelte/icons/flask-conical";
  import Plus from "@lucide/svelte/icons/plus";
  import Save from "@lucide/svelte/icons/save";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Zap from "@lucide/svelte/icons/zap";
  import { onMount } from "svelte";

  import {
    createAutomationRule,
    deleteAutomationRule,
    emptyAutomationRuleDraft,
    getRowsByIds,
    listAutomationRules,
    listGroups,
    previewAutomationRule,
    previewAutomationRuleDraft,
    reorderAutomationRules,
    runAutomationRuleOnLibrary,
    setAutomationRuleEnabled,
    updateAutomationRule,
    type AutomationRule,
    type AutomationRuleDraft,
    type GroupSummary,
    type RowRecord,
    type RuleAction,
    type RuleCondition,
    type RuleExecutionSummary,
    type RulePreview,
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
  import { focusMainWindow, type ToolboxRowRequest } from "../../windows/toolbox";
  import RuleActionEditor from "./RuleActionEditor.svelte";
  import RuleConditionEditor from "./RuleConditionEditor.svelte";

  const initialDraft = emptyAutomationRuleDraft();

  let rules = $state<AutomationRule[]>([]);
  let groups = $state<GroupSummary[]>([]);
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
  let newActionType = $state<RuleAction["type"]>("addTags");
  let openingRowId = $state<number | null>(null);

  const actionOptions: [RuleAction["type"], string][] = [
    ["addTags", "添加 Tag"], ["removeTags", "移除 Tag"], ["setGroup", "移入分组"],
    ["clearGroup", "清除分组"], ["appendPrompt", "追加提示词"],
    ["deletePromptTags", "删除指定提示词"], ["replacePrompt", "查找替换提示词"],
    ["prefixArtist", "修正 artist: 前缀"], ["setNote", "设置备注"],
    ["appendNote", "追加备注"], ["clearNote", "清空备注"],
    ["stopProcessing", "停止后续规则"],
  ];

  const selectedRule = $derived(rules.find(rule => rule.id === selectedId) ?? null);
  const dirty = $derived(JSON.stringify(draft) !== JSON.stringify(baseline));
  const conditionCount = $derived(
    draft.conditions.groups.reduce((sum, group) => sum + group.conditions.length, 0),
  );

  onMount(() => {
    void initialize();
    // 规则草稿有未保存修改时，拦截关窗
    return registerCloseGuard(() =>
      dirty ? `自动规则「${draft.name.trim() || "未命名规则"}」有未保存的修改` : null,
    );
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
      [rules, groups] = await Promise.all([listAutomationRules(), listGroups()]);
      if (rules.length > 0) loadRule(rules[0]);
      else startNew(false);
    } catch (cause) {
      error = errorText(cause);
    } finally {
      loading = false;
    }
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
      case "setGroup": return { type, groupId: groups[0]?.id ?? 0, onlyIfUngrouped: false };
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
    draft.actions.push(defaultAction(newActionType));
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
  <div class="center-state">正在读取规则…</div>
{:else}
  <div class="rules-layout">
    <aside class="rules-sidebar">
      <div class="sidebar-head">
        <div><strong>规则列表</strong><span>{rules.length} 条</span></div>
        <button type="button" class="btn btn-primary compact" onclick={() => startNew()}><Plus size={15} />新建</button>
      </div>

      {#if rules.length === 0}
        <div class="empty-rules"><Zap size={24} /><strong>还没有规则</strong><p>新建一条规则后，导入图片时就能自动整理。</p></div>
      {:else}
        <div class="rule-list">
          {#each rules as rule, index (rule.id)}
            <article class:is-selected={selectedId === rule.id} class:is-disabled={!rule.enabled}>
              <button type="button" class="rule-main" onclick={() => loadRule(rule, true)}>
                <strong>{rule.name}</strong><span>{ruleSubtitle(rule)}</span>
              </button>
              <div class="rule-controls">
                <label title={rule.enabled ? "已启用" : "已停用"}><input type="checkbox" checked={rule.enabled} onchange={event => void toggleRule(rule, (event.currentTarget as HTMLInputElement).checked)} /><span>{rule.enabled ? "启用" : "停用"}</span></label>
                <button type="button" title="上移" disabled={index === 0} onclick={() => void moveRule(index, -1)}><ArrowUp size={14} /></button>
                <button type="button" title="下移" disabled={index === rules.length - 1} onclick={() => void moveRule(index, 1)}><ArrowDown size={14} /></button>
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
        <section class="editor-section basics">
          <div class="section-title"><span>1</span><div><h3>名称与触发时机</h3><p>说明这条规则何时参与自动处理。</p></div></div>
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

        <section class="editor-section">
          <div class="section-title"><span>2</span><div><h3>条件检查</h3><p>每张图片单独判断；条件组可以表达 AND 与 OR。</p></div></div>
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

        <section class="editor-section">
          <div class="section-title"><span>3</span><div><h3>执行任务</h3><p>任务按从上到下的顺序执行；后续规则能看到这些修改。</p></div></div>
          <div class="action-list">
            {#each draft.actions as action, index (`action-${index}`)}
              <RuleActionEditor {action} {groups} onreplace={value => replaceAction(index, value)} onremove={() => removeAction(index)} onmoveup={() => moveAction(index, -1)} onmovedown={() => moveAction(index, 1)} canmoveup={index > 0} canmovedown={index < draft.actions.length - 1} />
            {/each}
          </div>
          <div class="add-action"><select bind:value={newActionType}>{#each actionOptions as item}<option value={item[0]}>{item[1]}</option>{/each}</select><button type="button" class="btn" onclick={addAction}><Plus size={15} />添加任务</button></div>
        </section>

        <section class="editor-section test-section">
          <div class="section-title"><span>4</span><div><h3>测试与应用</h3><p>测试只读取资料库，未保存的草稿也能直接测试；应用现有图片前会再次确认。</p></div></div>
          {#if selectedId === null || dirty}
            <p class="test-hint">当前是{selectedId === null ? "未保存的新规则" : "有未保存修改的规则"}：可以直接测试查看命中效果；“应用到现有图片”与导入时自动执行需要先保存。</p>
          {/if}
          <div class="test-actions">
            <button type="button" class="btn" disabled={testing || running} onclick={() => void testRule()}><FlaskConical size={15} />{testing ? "测试中…" : "测试现有资料库"}</button>
            <button type="button" class="btn btn-primary" disabled={!preview || selectedId === null || dirty || running || preview.rowsNeedingChanges === 0} onclick={() => void runOnLibrary()}><Zap size={15} />{running ? "执行中…" : "应用到现有图片"}</button>
          </div>

          {#if preview}
            <div class="preview-summary"><div><span>扫描</span><strong>{formatCount(preview.scannedRows)}</strong></div><div><span>命中</span><strong>{formatCount(preview.matchedRows)}</strong></div><div><span>需要修改</span><strong>{formatCount(preview.rowsNeedingChanges)}</strong></div><div><span>停止后续</span><strong>{formatCount(preview.stoppedRows)}</strong></div></div>
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
{/if}

<style>
  .center-state { height: 100%; display: grid; place-items: center; color: var(--text-3); }
  .rules-layout { height: 100%; min-height: 0; display: grid; grid-template-columns: 292px minmax(0, 1fr); }
  .rules-sidebar { min-height: 0; overflow-y: auto; padding: 18px 12px; border-right: 1px solid var(--border); background: var(--surface); }
  .sidebar-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 0 6px 14px; }
  .sidebar-head > div { display: flex; align-items: baseline; gap: 7px; }
  .sidebar-head strong { font-size: var(--font-md); }
  .sidebar-head span { color: var(--text-3); font-size: var(--font-xs); }
  .compact { min-height: 32px; padding: 5px 9px; }
  .btn { display: inline-flex; align-items: center; justify-content: center; gap: 6px; }
  .empty-rules { min-height: 210px; display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 20px; text-align: center; color: var(--text-3); }
  .empty-rules strong { margin-top: 10px; color: var(--text-2); }
  .empty-rules p { max-width: 210px; margin-top: 5px; font-size: var(--font-sm); line-height: 1.55; }
  .rule-list { display: grid; gap: 7px; }
  .rule-list article { border: 1px solid transparent; border-radius: var(--radius-s); background: var(--surface-2); overflow: hidden; }
  .rule-list article.is-selected { border-color: var(--accent); background: var(--accent-soft); }
  .rule-list article.is-disabled { opacity: .65; }
  .rule-main { width: 100%; display: grid; gap: 3px; padding: 10px; border: 0; background: transparent; text-align: left; color: var(--text); }
  .rule-main strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--font-sm); }
  .rule-main span { color: var(--text-3); font-size: var(--font-xs); }
  .rule-controls { display: flex; align-items: center; justify-content: flex-end; gap: 3px; padding: 0 7px 7px; }
  .rule-controls label { margin-right: auto; display: flex; align-items: center; gap: 5px; color: var(--text-3); font-size: var(--font-xs); }
  .rule-controls button { width: 27px; height: 25px; display: grid; place-items: center; border: 0; border-radius: 5px; background: transparent; color: var(--text-3); }
  .rule-controls button:hover:not(:disabled) { background: var(--surface-3); color: var(--text); }
  .rule-controls button:disabled { opacity: .25; }
  .rule-editor { min-width: 0; min-height: 0; overflow-y: auto; background: var(--bg); }
  .editor-head { position: sticky; top: 0; z-index: 5; min-height: 76px; display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 14px 24px; border-bottom: 1px solid var(--border); background: var(--surface); }
  .eyebrow { color: var(--text-3); font-size: var(--font-xs); }
  .editor-head h2 { margin-top: 2px; font-size: var(--font-lg); }
  .editor-actions { display: flex; gap: 8px; }
  .danger-ghost { color: var(--danger, #c53d4a); }
  .error-banner { margin: 16px 24px 0; padding: 10px 12px; border: 1px solid color-mix(in srgb, #d1495b 35%, var(--border)); border-radius: var(--radius-s); background: color-mix(in srgb, #d1495b 9%, var(--surface)); color: #c83d51; font-size: var(--font-sm); }
  .editor-body { max-width: 980px; display: grid; gap: 16px; padding: 20px 24px 42px; }
  .editor-section { padding: 18px; border: 1px solid var(--border); border-radius: var(--radius-m); background: var(--surface); }
  .section-title { display: flex; align-items: flex-start; gap: 11px; margin-bottom: 15px; }
  .section-title > span { width: 25px; height: 25px; display: grid; place-items: center; flex: none; border-radius: 50%; background: var(--accent-soft); color: var(--accent); font-size: var(--font-xs); font-weight: 700; }
  .section-title h3 { font-size: var(--font-md); }
  .section-title p { margin-top: 2px; color: var(--text-3); font-size: var(--font-sm); }
  .field-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  label { display: grid; gap: 4px; }
  label > span { color: var(--text-3); font-size: var(--font-xs); }
  input { min-height: 34px; padding: 6px 9px; border: 1px solid var(--border); border-radius: var(--radius-s); background: var(--surface); color: var(--text); font: inherit; }
  .trigger-row { display: flex; flex-wrap: wrap; gap: 9px 18px; margin-top: 13px; }
  .trigger-row label { display: flex; align-items: center; gap: 7px; color: var(--text-2); font-size: var(--font-sm); }
  .trigger-row input { min-height: 0; }
  .logic-toolbar { display: flex; align-items: end; flex-wrap: wrap; gap: 10px; padding: 11px; border-radius: var(--radius-s); background: var(--surface-2); }
  .logic-summary { min-height: 34px; display: inline-flex; align-items: center; margin-left: auto; color: var(--text-3); font-size: var(--font-sm); }
  .condition-groups { display: grid; gap: 12px; margin: 12px 0; }
  .condition-group { padding: 12px; border: 1px dashed color-mix(in srgb, var(--accent) 35%, var(--border)); border-radius: var(--radius-m); background: color-mix(in srgb, var(--accent-soft) 25%, var(--surface)); }
  .condition-group > header { display: flex; align-items: center; justify-content: space-between; gap: 10px; margin-bottom: 9px; }
  .condition-group > header > div { display: flex; align-items: center; gap: 10px; }
  .condition-group > header strong { font-size: var(--font-sm); }
  .text-danger { border: 0; background: transparent; color: var(--danger, #c53d4a); font-size: var(--font-xs); }
  .text-danger:disabled { opacity: .35; }
  .condition-list, .action-list { display: grid; gap: 9px; }
  .add-inline { display: inline-flex; align-items: center; gap: 5px; margin-top: 9px; padding: 5px 8px; border: 0; border-radius: 6px; background: transparent; color: var(--accent); font-size: var(--font-sm); }
  .add-inline:hover { background: var(--accent-soft); }
  .add-action { display: flex; align-items: center; gap: 8px; margin-top: 11px; }
  .test-actions { display: flex; gap: 8px; }
  .test-hint { margin-bottom: 10px; color: var(--text-3); font-size: var(--font-sm); }
  .preview-summary { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 8px; margin-top: 14px; }
  .preview-summary div { display: grid; gap: 2px; padding: 10px; border-radius: var(--radius-s); background: var(--surface-2); }
  .preview-summary span { color: var(--text-3); font-size: var(--font-xs); }
  .preview-summary strong { font-size: var(--font-lg); }
  .result-empty { margin-top: 10px; color: var(--text-3); font-size: var(--font-sm); }
  .execution-result { display: flex; gap: 9px; margin-top: 12px; padding: 12px; border-radius: var(--radius-s); background: color-mix(in srgb, #2aa876 10%, var(--surface)); color: #25855f; }
  .execution-result p { margin-top: 3px; color: var(--text-2); font-size: var(--font-xs); }
  .execution-result .error-line { color: var(--danger, #c53d4a); }
  .sample-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(82px, 1fr)); gap: 8px; margin-top: 12px; }
  .sample-grid button { min-width: 0; display: grid; gap: 4px; padding: 5px; border: 1px solid var(--border); border-radius: var(--radius-s); background: var(--surface-2); color: var(--text-3); font-size: var(--font-xs); }
  .sample-grid :global(.thumbnail-stack) { width: 100%; aspect-ratio: 1; border-radius: 6px; overflow: hidden; }
  @media (max-width: 920px) { .rules-layout { grid-template-columns: 230px minmax(0, 1fr); } .field-grid { grid-template-columns: 1fr; } .preview-summary { grid-template-columns: 1fr 1fr; } }
</style>
