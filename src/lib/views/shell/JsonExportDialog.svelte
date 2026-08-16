<script lang="ts">
  import X from "@lucide/svelte/icons/x";

  import type { JsonExportOptions } from "../../api";
  import { executeJsonExport } from "../../stores/export-actions";
  import {
    closeJsonExportDialog,
    jsonExportDialog,
  } from "../../stores/json-export-dialog.svelte";
  import Modal from "../../ui/Modal.svelte";

  let noteNumberNames = $state(true);
  let includeArtists = $state(true);
  let deduplicate = $state(true);
  let working = $state(false);

  async function startExport(): Promise<void> {
    const selection = jsonExportDialog.selection;
    if (!selection || working) return;

    const options: JsonExportOptions = {
      noteNumberNames,
      includeArtists,
      deduplicate,
    };
    working = true;
    await executeJsonExport(selection, options);
    working = false;
    closeJsonExportDialog();
  }
</script>

<Modal
  open={jsonExportDialog.selection !== null}
  onclose={closeJsonExportDialog}
  busy={working}
  width="460px"
  labelledby="json-export-title"
>
  <div class="dialog-content">
    <header>
      <div>
        <h3 id="json-export-title">导出智绘姬 JSON</h3>
        <p>导出{jsonExportDialog.scopeLabel}</p>
      </div>
      <button
        type="button"
        class="close-btn"
        disabled={working}
        aria-label="关闭"
        onclick={closeJsonExportDialog}
      ><X size={16} strokeWidth={2} /></button>
    </header>

    <div class="body">
      <label class="option-row">
        <input type="checkbox" bind:checked={noteNumberNames} disabled={working} />
        <span>
          <strong>名称使用“备注_序号”</strong>
          <small>例如“夏日白裙_1”；没有备注时只使用数字序号</small>
        </span>
      </label>

      <label class="option-row">
        <input type="checkbox" bind:checked={includeArtists} disabled={working} />
        <span>
          <strong>补齐画师串</strong>
          <small>把资料库中已有、但正向提示词里缺少的画师追加到末尾</small>
        </span>
      </label>

      <label class="option-row">
        <input type="checkbox" bind:checked={deduplicate} disabled={working} />
        <span>
          <strong>按最终正向提示词去重</strong>
          <small>补齐画师后再比较；重复时优先保留有备注的记录</small>
        </span>
      </label>

      <div class="example">
        执行顺序：补齐画师 → 去重 → 连续编号
      </div>
    </div>

    <footer>
      <button type="button" class="btn" disabled={working} onclick={closeJsonExportDialog}>取消</button>
      <button
        type="button"
        class="btn btn-primary"
        disabled={working}
        onclick={() => void startExport()}
      >{working ? "导出中…" : "选择位置并导出"}</button>
    </footer>
  </div>
</Modal>

<style>
  .dialog-content {
    display: flex;
    min-height: 0;
    flex-direction: column;
  }

  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 15px 17px 13px;
    border-bottom: 1px solid var(--border);
  }

  header h3 {
    font-size: var(--font-base);
    font-weight: 650;
  }

  header p {
    margin-top: 3px;
    color: var(--text-3);
    font-size: var(--font-sm);
  }

  .close-btn {
    display: grid;
    flex: 0 0 auto;
    width: 28px;
    height: 28px;
    place-items: center;
    border: none;
    border-radius: var(--radius-s);
    background: transparent;
    color: var(--text-2);
    cursor: pointer;
  }

  .close-btn:hover:not(:disabled) {
    background: var(--surface-2);
  }

  .body {
    display: grid;
    gap: 9px;
    padding: 14px 17px;
  }

  .option-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 11px;
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    background: var(--surface-2);
    cursor: pointer;
  }

  .option-row input {
    flex: 0 0 auto;
    margin-top: 2px;
  }

  .option-row span {
    display: grid;
    gap: 3px;
  }

  .option-row strong {
    color: var(--text-1);
    font-size: var(--font-md);
    font-weight: 600;
  }

  .option-row small,
  .example {
    color: var(--text-3);
    font-size: var(--font-sm);
    line-height: 1.5;
  }

  .example {
    padding: 3px 2px 0;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 17px;
    border-top: 1px solid var(--border);
  }
</style>
