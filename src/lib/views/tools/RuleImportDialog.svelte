<script lang="ts">
  import AlertTriangle from "@lucide/svelte/icons/alert-triangle";
  import FileJson from "@lucide/svelte/icons/file-json";
  import FolderTree from "@lucide/svelte/icons/folder-tree";
  import Tags from "@lucide/svelte/icons/tags";

  import type { AutomationRuleImportInspection } from "../../api";
  import Modal from "../../ui/Modal.svelte";

  let {
    inspection,
    fileName,
    busy,
    onclose,
    onconfirm,
  }: {
    inspection: AutomationRuleImportInspection;
    fileName: string;
    busy: boolean;
    onclose: () => void;
    onconfirm: () => void;
  } = $props();

  function triggers(runOnImport: boolean, runOnUpdate: boolean): string {
    const values = [runOnImport && "导入", runOnUpdate && "更新"].filter(Boolean);
    return values.length > 0 ? values.join("＋") : "仅手动";
  }
</script>

<Modal open={true} {onclose} {busy} labelledby="rule-import-title" width="620px">
  <header class="dialog-head">
    <div class="title-icon"><FileJson size={21} aria-hidden="true" /></div>
    <div>
      <h2 id="rule-import-title">确认导入规则</h2>
      <p title={fileName}>{fileName}</p>
    </div>
  </header>

  <div class="dialog-body">
    <div class="summary">
      <strong>{inspection.ruleCount} 条规则</strong>
      <span>JSON 格式版本 {inspection.version}</span>
    </div>

    <div class="rule-preview" aria-label="待导入规则">
      {#each inspection.rules as rule, index (`${rule.name}-${index}`)}
        <article>
          <div class="rule-index tabular">{index + 1}</div>
          <div class="rule-copy">
            <strong>{rule.importedName}</strong>
            {#if rule.importedName !== rule.name}
              <span class="renamed">原名“{rule.name}”，因重名自动改名</span>
            {/if}
            <span>{triggers(rule.runOnImport, rule.runOnUpdate)} · {rule.conditionCount} 个条件 · {rule.actionCount} 个任务</span>
          </div>
        </article>
      {/each}
    </div>

    {#if inspection.missingTags.length > 0 || inspection.missingGroups.length > 0}
      <section class="dependencies">
        <h3>将随规则创建的项目</h3>
        <p>这些名称来自规则文件；确认导入后才会写入当前资料库。</p>
        {#if inspection.missingTags.length > 0}
          <div class="dependency-row">
            <span class="dependency-label"><Tags size={15} aria-hidden="true" />Tag（{inspection.missingTags.length}）</span>
            <div class="chips">
              {#each inspection.missingTags.slice(0, 12) as name (name)}<span>{name}</span>{/each}
              {#if inspection.missingTags.length > 12}<span>＋{inspection.missingTags.length - 12}</span>{/if}
            </div>
          </div>
        {/if}
        {#if inspection.missingGroups.length > 0}
          <div class="dependency-row">
            <span class="dependency-label"><FolderTree size={15} aria-hidden="true" />分组（{inspection.missingGroups.length}）</span>
            <div class="chips">
              {#each inspection.missingGroups.slice(0, 12) as name (name)}<span>{name}</span>{/each}
              {#if inspection.missingGroups.length > 12}<span>＋{inspection.missingGroups.length - 12}</span>{/if}
            </div>
          </div>
        {/if}
      </section>
    {/if}

    <div class="safety-note">
      <span class="safety-icon"><AlertTriangle size={17} aria-hidden="true" /></span>
      <p><strong>导入后的规则全部保持停用。</strong>请逐条检查条件和任务，确认无误后再手动启用。</p>
    </div>
  </div>

  <footer class="dialog-actions">
    <button type="button" class="btn" disabled={busy} onclick={onclose}>取消</button>
    <button type="button" class="btn btn-primary" disabled={busy} onclick={onconfirm}>
      {busy ? "导入中…" : `导入 ${inspection.ruleCount} 条规则`}
    </button>
  </footer>
</Modal>

<style>
  .dialog-head { display: flex; align-items: center; gap: 11px; padding: 18px 20px 14px; border-bottom: 1px solid var(--border); }
  .title-icon { width: 38px; height: 38px; display: grid; place-items: center; flex: none; border-radius: 10px; background: var(--accent-soft); color: var(--accent); }
  .dialog-head h2 { font-size: var(--font-lg); }
  .dialog-head p { max-width: 480px; margin-top: 2px; overflow: hidden; color: var(--text-3); font-size: var(--font-xs); text-overflow: ellipsis; white-space: nowrap; }
  .dialog-body { min-height: 0; overflow-y: auto; display: grid; gap: 12px; padding: 16px 20px; }
  .summary { display: flex; align-items: baseline; gap: 8px; }
  .summary strong { font-size: var(--font-md); }
  .summary span { color: var(--text-3); font-size: var(--font-xs); }
  .rule-preview { max-height: 220px; overflow-y: auto; display: grid; gap: 2px; padding: 5px; border: 1px solid var(--border); border-radius: var(--radius-m); background: var(--surface-2); }
  .rule-preview article { display: flex; align-items: center; gap: 9px; padding: 8px 9px; border-radius: 7px; background: var(--surface); }
  .rule-index { width: 20px; flex: none; color: var(--text-4); font-size: var(--font-xs); text-align: center; }
  .rule-copy { min-width: 0; display: grid; gap: 1px; }
  .rule-copy strong { overflow: hidden; font-size: var(--font-sm); text-overflow: ellipsis; white-space: nowrap; }
  .rule-copy > span { color: var(--text-3); font-size: var(--font-xs); }
  .rule-copy .renamed { color: var(--warning); }
  .dependencies { display: grid; gap: 8px; padding: 11px; border: 1px solid var(--border); border-radius: var(--radius-m); }
  .dependencies h3 { font-size: var(--font-sm); }
  .dependencies > p { margin-top: -5px; color: var(--text-3); font-size: var(--font-xs); }
  .dependency-row { display: grid; gap: 5px; }
  .dependency-label { display: inline-flex; align-items: center; gap: 5px; color: var(--text-2); font-size: var(--font-xs); font-weight: 600; }
  .chips { display: flex; flex-wrap: wrap; gap: 5px; }
  .chips span { padding: 3px 7px; border-radius: var(--radius-full); background: var(--surface-2); color: var(--text-2); font-size: var(--font-xs); }
  .safety-note { display: flex; align-items: flex-start; gap: 8px; padding: 10px 11px; border-radius: var(--radius-s); background: var(--warning-soft); color: var(--warning); }
  .safety-icon { margin-top: 1px; display: grid; place-items: center; flex: none; }
  .safety-note p { font-size: var(--font-sm); line-height: 1.5; }
  .dialog-actions { display: flex; justify-content: flex-end; gap: 8px; padding: 12px 20px 16px; border-top: 1px solid var(--border); }
</style>
