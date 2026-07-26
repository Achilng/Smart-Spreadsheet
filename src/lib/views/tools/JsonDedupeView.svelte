<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { open, save } from "@tauri-apps/plugin-dialog";

  import {
    dedupeZhihuijiJson,
    inspectZhihuijiJson,
    type JsonDedupeInspection,
    type JsonDedupeProgress,
    type JsonDedupeSummary,
  } from "../../api";
  import { errorText, formatCount } from "../../stores/app-state.svelte";
  import { softFade, softFly } from "../../ui/motion";

  let inputPath = $state<string | null>(null);
  let inspection = $state<JsonDedupeInspection | null>(null);
  let summary = $state<JsonDedupeSummary | null>(null);
  let progress = $state<JsonDedupeProgress | null>(null);
  let working = $state(false);
  let error = $state<string | null>(null);

  const fileName = $derived(inputPath?.split(/[\\/]/).pop() ?? null);

  async function pickInput(): Promise<void> {
    const selection = await open({
      multiple: false,
      directory: false,
      title: "选择智绘姬 JSON 文件",
      filters: [{ name: "JSON 文件", extensions: ["json"] }],
    });
    if (typeof selection !== "string") {
      return;
    }
    working = true;
    error = null;
    summary = null;
    inspection = null;
    inputPath = selection;
    try {
      inspection = await inspectZhihuijiJson(selection);
    } catch (cause) {
      error = errorText(cause);
      inputPath = null;
    } finally {
      working = false;
    }
  }

  async function runDedupe(): Promise<void> {
    if (!inputPath || !inspection || working) {
      return;
    }
    const defaultName = (fileName ?? "presets.json").replace(/\.json$/i, "_去重.json");
    const outputPath = await save({
      title: "保存去重后的 JSON（不能与输入文件相同）",
      defaultPath: defaultName,
      filters: [{ name: "JSON 文件", extensions: ["json"] }],
    });
    if (typeof outputPath !== "string") {
      return;
    }
    working = true;
    error = null;
    summary = null;
    const unlisten = await listen<JsonDedupeProgress>("json-dedupe://progress", event => {
      progress = event.payload;
    });
    try {
      summary = await dedupeZhihuijiJson(inputPath, outputPath);
    } catch (cause) {
      error = errorText(cause);
    } finally {
      unlisten();
      progress = null;
      working = false;
    }
  }
</script>

<div class="json-page">
  <div class="json-shell tool-card">
    <div class="json-body">
      <p class="faint">
        按 fixedPrompt 去除重复预设并重新连续编号，原文件不会被修改。
      </p>

      <div class="file-row">
        <button type="button" class="btn" disabled={working} onclick={() => void pickInput()}>
          选择 JSON 文件
        </button>
        {#if fileName}
          <span class="file-name" title={inputPath} transition:softFly={{ duration: 145, y: 4 }}>{fileName}</span>
        {/if}
      </div>

      {#if error}
        <p class="error-text" transition:softFly={{ duration: 150, y: 4 }}>{error}</p>
      {/if}

      {#if inspection}
        <div class="stats metric-grid tabular" style:--metric-cols="3" transition:softFly={{ duration: 165, y: 4 }}>
          <div>
            <strong>{formatCount(inspection.originalCount)}</strong>
            <span>预设总数</span>
          </div>
          <div class:is-highlight={inspection.duplicateCount > 0}>
            <strong>{formatCount(inspection.duplicateCount)}</strong>
            <span>重复</span>
          </div>
          <div>
            <strong>{formatCount(inspection.uniqueCount)}</strong>
            <span>去重后</span>
          </div>
        </div>

        {#if inspection.preview.length > 0}
          <div class="preview" transition:softFade={{ duration: 150 }}>
            <p class="faint">内容预览（前 {inspection.preview.length} 条）：</p>
            {#each inspection.preview as item (item.presetKey)}
              <div class="preview-item">
                <span class="preview-key">#{item.presetKey}</span>
                <span class="preview-prompt" title={item.fixedPrompt}>
                  {item.fixedPrompt || "（空 fixedPrompt）"}
                </span>
              </div>
            {/each}
          </div>
        {/if}

        {#if progress}
          <div class="dedupe-progress" role="status" transition:softFade={{ duration: 130 }}>
            <p class="faint">
              正在去重 {formatCount(progress.processed)} / {formatCount(progress.total)}，已发现重复
              {formatCount(progress.duplicateCount)} 条…
            </p>
            <span class="progress" role="progressbar" aria-valuemin={0} aria-valuemax={progress.total} aria-valuenow={progress.processed}>
              <span class="progress-fill" style:transform="scaleX({progress.total > 0 ? progress.processed / progress.total : 0})"></span>
            </span>
          </div>
        {/if}

        {#if summary}
          <p class="success-text" role="status" transition:softFly={{ duration: 160, y: 4 }}>
            完成：去除 {formatCount(summary.duplicateCount)} 条重复，保留
            {formatCount(summary.uniqueCount)} 条，已保存到 {summary.outputPath}
          </p>
        {/if}
      {/if}
    </div>

    <footer class="json-footer">
      {#if inspection}
        <span class="faint">
          {inspection.duplicateCount > 0
            ? `可去除 ${formatCount(inspection.duplicateCount)} 条重复预设`
            : "没有重复预设，无需去重"}
        </span>
      {:else}
        <span class="faint">请先选择文件</span>
      {/if}
      <button
        type="button"
        class="btn btn-primary"
        disabled={working || !inspection || inspection.duplicateCount === 0}
        onclick={() => void runDedupe()}
      >
        {#if working}<span class="spinner" aria-hidden="true"></span>{/if}
        {working ? "处理中…" : "去重并另存为…"}
      </button>
    </footer>
  </div>
</div>

<style>
  .json-page {
    min-height: 100%;
    padding: 24px 28px 32px;
  }

  .json-shell {
    width: min(680px, 100%);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .json-body {
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow-y: auto;
    font-size: var(--font-md);
  }

  .json-body p {
    margin: 0;
  }

  .file-row {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .file-name {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-2);
  }

  .preview {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .preview-item {
    display: flex;
    gap: 8px;
    align-items: baseline;
    min-width: 0;
  }

  .preview-key {
    flex: none;
    font-size: var(--font-xs);
    color: var(--text-3);
  }

  .preview-prompt {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-2);
  }

  .dedupe-progress {
    display: grid;
    gap: 7px;
  }

  .json-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 20px;
    border-top: 1px solid var(--border);
    background: var(--surface-2);
    font-size: var(--font-md);
  }

  .error-text {
    color: var(--danger);
  }

  .success-text {
    color: var(--success);
  }
</style>
