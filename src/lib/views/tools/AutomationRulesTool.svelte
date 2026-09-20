<script lang="ts">
  import { formatCount } from "../../utils/format";
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
  import { flip } from "svelte/animate";
  import { flipDuration } from "../../ui/motion";
  import Thumbnail from "../../ui/Thumbnail.svelte";
  import Dropdown from "../../ui/Dropdown.svelte";
  import RuleActionEditor from "./RuleActionEditor.svelte";
  import RuleConditionEditor from "./RuleConditionEditor.svelte";
  import RuleImportDialog from "./RuleImportDialog.svelte";
  import RuleTextImportDialog from "./RuleTextImportDialog.svelte";
  import { createAutomationRulesController } from "../../features/automation/controller.svelte";

  const controller = createAutomationRulesController();
</script>

{#if controller.loading}
  <div class="center-state empty-state"><span class="spinner" aria-hidden="true"></span>正在读取规则…</div>
{:else}
  <div class="rules-layout">
    <aside class="rules-sidebar">
      <div class="sidebar-head">
        <div><strong>规则列表</strong><span>{controller.rules.length} 条</span></div>
        <button type="button" class="btn btn-primary compact" onclick={() => controller.startNew()}><Plus size={15} />新建</button>
      </div>
      <div class="sidebar-transfer">
        <button type="button" class="btn compact" disabled={controller.transferring} onclick={() => void controller.chooseRuleImport()}><ImportIcon size={14} />导入 JSON</button>
        <Dropdown label="导出 JSON" items={controller.exportItems} disabled={controller.transferring || controller.rules.length === 0} />
        <button type="button" class="btn compact wide-transfer" disabled={controller.transferring} onclick={controller.openRuleTextImport}><ClipboardPaste size={14} />粘贴 JSON 文本</button>
        <button type="button" class="btn compact ai-prompt-copy" disabled={controller.transferring || controller.copyingPrompt} onclick={() => void controller.copyAiRulePrompt()}><ClipboardCopy size={14} />{controller.copyingPrompt ? "正在准备…" : "复制 AI 编写提示词"}</button>
      </div>

      {#if controller.rules.length === 0}
        <div class="empty-rules empty-state"><Zap size={24} /><strong>还没有规则</strong><p>新建一条规则后，导入图片时就能自动整理。</p></div>
      {:else}
        <div class="rule-list">
          {#each controller.rules as rule, index (rule.id)}
            <article
              class:is-selected={controller.selectedId === rule.id}
              class:is-disabled={!rule.enabled}
              animate:flip={{ duration: flipDuration(170) }}
            >
              <button type="button" class="rule-main" onclick={() => controller.loadRule(rule, true)}>
                <span class="rule-order tabular" aria-hidden="true">{index + 1}</span>
                <span class="rule-copy">
                  <strong>{rule.name || "未命名规则"}</strong>
                  <span class="rule-sub">{controller.ruleSubtitle(rule)}</span>
                </span>
              </button>
              <div class="rule-side">
                <div class="rule-move">
                  <button type="button" title="上移" disabled={index === 0} onclick={() => void controller.moveRule(index, -1)}><ArrowUp size={13} /></button>
                  <button type="button" title="下移" disabled={index === controller.rules.length - 1} onclick={() => void controller.moveRule(index, 1)}><ArrowDown size={13} /></button>
                </div>
                <input
                  type="checkbox"
                  class="switch"
                  title={rule.enabled ? "已启用，点击停用" : "已停用，点击启用"}
                  aria-label={`${rule.enabled ? "停用" : "启用"}规则「${rule.name}」`}
                  checked={rule.enabled}
                  onchange={event => void controller.toggleRule(rule, (event.currentTarget as HTMLInputElement).checked)}
                />
              </div>
            </article>
          {/each}
        </div>
      {/if}
    </aside>

    <main class="rule-editor">
      <header class="editor-head">
        <div><span class="eyebrow">{controller.selectedId === null ? "新规则" : `规则 #${controller.selectedId}`}</span><h2>{controller.draft.name.trim() || "未命名规则"}</h2></div>
        <div class="editor-actions">
          {#if controller.selectedRule}<button type="button" class="btn danger-ghost" disabled={controller.deleting} onclick={() => void controller.removeSelectedRule()}><Trash2 size={15} />删除</button>{/if}
          <button type="button" class="btn btn-primary" disabled={controller.saving || !controller.dirty} onclick={() => void controller.saveRule()}><Save size={15} />{controller.saving ? "保存中…" : "保存规则"}</button>
        </div>
      </header>

      {#if controller.error}<div class="error-banner" role="alert">{controller.error}</div>{/if}

      <div class="editor-body">
        <section class="editor-section basics tool-card">
          <div class="section-title"><span class="step-badge">1</span><div><h3>名称与触发时机</h3><p>说明这条规则何时参与自动处理。</p></div></div>
          <div class="field-grid">
            <label><span>规则名称</span><input value={controller.draft.name} placeholder="例如：识别某个角色" oninput={event => { controller.draft.name = (event.currentTarget as HTMLInputElement).value; controller.resetResult(); }} /></label>
            <label><span>说明（可选）</span><input value={controller.draft.description} placeholder="记录这条规则的用途" oninput={event => { controller.draft.description = (event.currentTarget as HTMLInputElement).value; controller.resetResult(); }} /></label>
          </div>
          <div class="trigger-row">
            <label><input type="checkbox" checked={controller.draft.enabled} onchange={event => { controller.draft.enabled = (event.currentTarget as HTMLInputElement).checked; controller.resetResult(); }} />启用规则</label>
            <label><input type="checkbox" checked={controller.draft.runOnImport} onchange={event => { controller.draft.runOnImport = (event.currentTarget as HTMLInputElement).checked; controller.resetResult(); }} />新图片导入后自动执行</label>
            <label><input type="checkbox" checked={controller.draft.runOnUpdate} onchange={event => { controller.draft.runOnUpdate = (event.currentTarget as HTMLInputElement).checked; controller.resetResult(); }} />更新现有图片后自动执行</label>
          </div>
        </section>

        <section class="editor-section tool-card">
          <div class="section-title"><span class="step-badge">2</span><div><h3>条件检查</h3><p>每张图片单独判断；条件组可以表达 AND 与 OR。</p></div></div>
          <div class="logic-toolbar">
            <label><span>条件组之间</span><select value={controller.draft.conditions.mode} onchange={event => { controller.draft.conditions.mode = (event.currentTarget as HTMLSelectElement).value as "all" | "any"; controller.resetResult(); }}><option value="any">满足任意一组（OR）</option><option value="all">必须满足全部组（AND）</option></select></label>
            <label><span>执行时机</span><select value={controller.draft.conditions.negate ? "notMatched" : "matched"} onchange={event => { controller.draft.conditions.negate = (event.currentTarget as HTMLSelectElement).value === "notMatched"; controller.resetResult(); }}><option value="matched">条件成立时执行</option><option value="notMatched">条件不成立时执行</option></select></label>
            <span class="logic-summary">共 {controller.draft.conditions.groups.length} 组、{controller.conditionCount} 个条件</span>
          </div>

          <div class="condition-groups">
            {#each controller.draft.conditions.groups as group, groupIndex (`group-${groupIndex}`)}
              <article class="condition-group">
                <header><div><strong>条件组 {groupIndex + 1}</strong><select value={group.mode} onchange={event => { group.mode = (event.currentTarget as HTMLSelectElement).value as "all" | "any"; controller.resetResult(); }}><option value="all">组内条件全部成立（AND）</option><option value="any">组内任意条件成立（OR）</option></select></div><button type="button" class="text-danger" disabled={controller.draft.conditions.groups.length === 1} onclick={() => controller.removeGroup(groupIndex)}>删除组</button></header>
                <div class="condition-list">
                  {#each group.conditions as condition, conditionIndex (`condition-${groupIndex}-${conditionIndex}`)}
                    <RuleConditionEditor {condition} groups={controller.groups} onreplace={value => controller.replaceCondition(groupIndex, conditionIndex, value)} onremove={() => controller.removeCondition(groupIndex, conditionIndex)} />
                  {/each}
                </div>
                <button type="button" class="add-inline" onclick={() => controller.addCondition(groupIndex)}><Plus size={14} />添加条件</button>
              </article>
            {/each}
          </div>
          <button type="button" class="btn" onclick={controller.addGroup}><Plus size={15} />添加条件组</button>
        </section>

        <section class="editor-section tool-card">
          <div class="section-title"><span class="step-badge">3</span><div><h3>执行任务</h3><p>任务按从上到下的顺序执行；后续规则能看到这些修改。</p></div></div>
          <div class="action-list">
            {#each controller.draft.actions as action, index (`action-${index}`)}
              <RuleActionEditor {action} groups={controller.groups} tags={controller.tags} tagsloading={controller.tagsLoading} onrefreshtags={controller.refreshTags} onreplace={value => controller.replaceAction(index, value)} onremove={() => controller.removeAction(index)} onmoveup={() => controller.moveAction(index, -1)} onmovedown={() => controller.moveAction(index, 1)} canmoveup={index > 0} canmovedown={index < controller.draft.actions.length - 1} />
            {/each}
          </div>
          <div class="add-action"><button type="button" class="btn" onclick={controller.addAction}><Plus size={15} />添加任务</button></div>
        </section>

        <section class="editor-section test-section tool-card">
          <div class="section-title"><span class="step-badge">4</span><div><h3>测试与应用</h3><p>测试只读取资料库，未保存的草稿也能直接测试；应用现有图片前会再次确认。</p></div></div>
          {#if controller.selectedId === null || controller.dirty}
            <p class="test-hint">当前是{controller.selectedId === null ? "未保存的新规则" : "有未保存修改的规则"}：可以直接测试查看命中效果；“应用到现有图片”与导入时自动执行需要先保存。</p>
          {/if}
          <div class="test-actions">
            <button type="button" class="btn" disabled={controller.testing || controller.running} onclick={() => void controller.testRule()}><FlaskConical size={15} />{controller.testing ? "测试中…" : "测试现有资料库"}</button>
            <button type="button" class="btn btn-primary" disabled={!controller.preview || controller.selectedId === null || controller.dirty || controller.running || controller.preview.rowsNeedingChanges === 0} onclick={() => void controller.runOnLibrary()}><Zap size={15} />{controller.running ? "执行中…" : "应用到现有图片"}</button>
          </div>

          {#if controller.preview}
            <div class="preview-summary metric-grid tabular"><div><span>扫描</span><strong>{formatCount(controller.preview.scannedRows)}</strong></div><div><span>命中</span><strong>{formatCount(controller.preview.matchedRows)}</strong></div><div><span>需要修改</span><strong>{formatCount(controller.preview.rowsNeedingChanges)}</strong></div><div><span>停止后续</span><strong>{formatCount(controller.preview.stoppedRows)}</strong></div></div>
            {#if controller.preview.matchedRows === 0}<div class="result-empty">没有图片命中当前规则。</div>{/if}
            {#if controller.preview.matchedRows > 0 && controller.preview.rowsNeedingChanges === 0}<div class="result-empty">当前规则不会对现有图片产生可保存的修改，无需手动应用。</div>{/if}
          {/if}

          {#if controller.execution}
            <div class="execution-result"><CheckCircle2 size={18} /><div><strong>执行完成，修改 {formatCount(controller.execution.changedRows)} 张图片</strong>{#each controller.execution.reports as report}<p class:error-line={Boolean(report.error)}>{report.ruleName}：命中 {formatCount(report.matchedRows)}，修改 {formatCount(report.changedRows)}{report.error ? `；失败：${report.error}` : ""}</p>{/each}</div></div>
          {/if}

          {#if controller.sampleRows.length > 0}
            <div class="sample-grid" aria-label="命中示例">
              {#each controller.sampleRows as row (row.id)}
                <button type="button" title="在主窗口查看" disabled={controller.openingRowId === row.id} onclick={() => void controller.openInMain(row.id)}><Thumbnail rowId={row.id} hasImage={Boolean(row.imagePath || row.storedImagePath)} alt={`规则命中图片 ${row.id}`} /><span>#{row.id}</span></button>
              {/each}
            </div>
          {/if}
        </section>
      </div>
    </main>
  </div>

  {#if controller.textImportOpen}
    <RuleTextImportDialog
      value={controller.importText}
      busy={controller.transferring}
      error={controller.textImportError}
      onchange={controller.updateRuleText}
      onclose={controller.closeRuleTextImport}
      oninspect={() => void controller.inspectRuleText()}
    />
  {/if}

  {#if controller.importInspection && controller.pendingImport}
    <RuleImportDialog
      inspection={controller.importInspection}
      sourceName={controller.pendingImport.kind === "file"
        ? controller.pendingImport.path.split(/[\\/]/).pop() ?? controller.pendingImport.path
        : "粘贴的 JSON 文本"}
      busy={controller.transferring}
      onclose={controller.closeRuleImport}
      onconfirm={() => void controller.confirmRuleImport()}
      onback={controller.pendingImport.kind === "text" ? controller.backToRuleText : undefined}
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
