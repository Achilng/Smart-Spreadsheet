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
  import { app, errorText, formatCount } from "../../stores/app-state.svelte";

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

<svelte:window
  onkeydown={event => {
    if (event.key === "Escape" && !working) {
      app.jsonDedupeOpen = false;
    }
  }}
/>

<div class="json-overlay">
  <div class="json-panel">
    <header class="json-header">
      <h3>智绘姬 JSON 去重</h3>
      <button
        type="button"
        class="btn"
        disabled={working}
        onclick={() => (app.jsonDedupeOpen = false)}
      >
        关闭
      </button>
    </header>

    <div class="json-body">
      <p class="faint">
        按 fixedPrompt 去除重复预设并重新连续编号，原文件不会被修改。
      </p>

      <div class="file-row">
        <button type="button" class="btn" disabled={working} onclick={() => void pickInput()}>
          选择 JSON 文件
        </button>
        {#if fileName}
          <span class="file-name" title={inputPath}>{fileName}</span>
        {/if}
      </div>

      {#if error}
        <p class="error-text">{error}</p>
      {/if}

      {#if inspection}
        <dl class="stats">
          <div>
            <dt>预设总数</dt>
            <dd>{formatCount(inspection.originalCount)}</dd>
          </div>
          <div>
            <dt>重复</dt>
            <dd class:highlight={inspection.duplicateCount > 0}>
              {formatCount(inspection.duplicateCount)}
            </dd>
          </div>
          <div>
            <dt>去重后</dt>
            <dd>{formatCount(inspection.uniqueCount)}</dd>
          </div>
        </dl>

        {#if inspection.preview.length > 0}
          <div class="preview">
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
          <p class="faint" role="status">
            正在去重 {formatCount(progress.processed)} / {formatCount(progress.total)}，已发现重复
            {formatCount(progress.duplicateCount)} 条…
          </p>
        {/if}

        {#if summary}
          <p class="success-text" role="status">
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
        {working ? "处理中…" : "去重并另存为…"}
      </button>
    </footer>
  </div>
</div>

<style>
  .json-overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: rgb(0 0 0 / 35%);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 40px;
  }

  .json-panel {
    width: min(560px, 100%);
    max-height: 100%;
    background: var(--bg);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-2);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .json-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    border-radius: var(--radius-m) var(--radius-m) 0 0;
  }

  .json-header h3 {
    font-size: 15px;
    margin: 0;
  }

  .json-body {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow-y: auto;
    font-size: 13px;
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

  .stats {
    display: flex;
    gap: 24px;
    margin: 0;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--surface);
  }

  .stats dt {
    font-size: 11px;
    color: var(--text-3);
  }

  .stats dd {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }

  .stats dd.highlight {
    color: var(--accent);
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
    font-size: 11px;
    color: var(--text-3);
  }

  .preview-prompt {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-2);
  }

  .json-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 16px;
    border-top: 1px solid var(--border);
    background: var(--surface);
    border-radius: 0 0 var(--radius-m) var(--radius-m);
    font-size: 13px;
  }

  .error-text {
    color: var(--danger);
  }

  .success-text {
    color: var(--success);
  }
</style>
