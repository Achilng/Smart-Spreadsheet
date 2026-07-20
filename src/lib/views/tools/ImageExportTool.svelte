<script lang="ts">
  import { emitTo, listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";

  import {
    exportSelectedImages,
    type ExportProgress,
    type ImageFileRenameMode,
    type ImageFilesExportResult,
  } from "../../api";
  import {
    errorText,
    formatCount,
    setNotice,
  } from "../../stores/app-state.svelte";
  import {
    focusMainWindow,
    type ToolboxSelectionSnapshot,
  } from "../../windows/toolbox";

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  let selectionSnapshot = $state<ToolboxSelectionSnapshot | null>(null);
  let destination = $state<string | null>(null);
  let renameEnabled = $state(false);
  let renameMode = $state<"random" | "custom">("random");
  let customName = $state("");
  let stripMetadata = $state(false);
  let exporting = $state(false);
  let progress = $state<ExportProgress | null>(null);
  let lastResult = $state<ImageFilesExportResult | null>(null);
  let localError = $state<string | null>(null);
  let selectionListenerReady = $state(false);

  const selectedCount = $derived(selectionSnapshot?.count ?? 0);
  const effectiveRenameMode = $derived<ImageFileRenameMode>(
    renameEnabled ? renameMode : "original",
  );
  const customNameValid = $derived(
    effectiveRenameMode !== "custom" || customName.trim().length > 0,
  );
  const canExport = $derived(
    !exporting &&
      selectedCount > 0 &&
      Boolean(destination) &&
      customNameValid,
  );

  onMount(() => {
    let disposed = false;
    let unlistenSelection: UnlistenFn | null = null;
    void listen<ToolboxSelectionSnapshot>("main://selection-changed", event => {
      selectionSnapshot = event.payload;
    }).then(unlisten => {
      if (disposed) {
        unlisten();
      } else {
        unlistenSelection = unlisten;
        selectionListenerReady = true;
        void requestSelection();
      }
    });

    return () => {
      disposed = true;
      unlistenSelection?.();
    };
  });

  $effect(() => {
    if (active && selectionListenerReady) {
      void requestSelection();
    }
  });

  async function requestSelection(): Promise<void> {
    try {
      await emitTo("main", "toolbox://request-selection");
    } catch {
      localError = "无法读取主窗口选区，请确认主窗口仍在运行。";
    }
  }

  async function chooseDestination(): Promise<void> {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "选择图片导出位置（将在其中创建新的输出文件夹）",
    });
    if (typeof selected !== "string") {
      return;
    }
    destination = selected;
    lastResult = null;
    localError = null;
  }

  async function returnToMain(): Promise<void> {
    try {
      await focusMainWindow();
    } catch (cause) {
      localError = `无法切换到主窗口：${errorText(cause)}`;
    }
  }

  async function runExport(): Promise<void> {
    if (!canExport || !selectionSnapshot || !destination) {
      return;
    }
    const selection = selectionSnapshot.selection;
    const target = destination;
    const custom = effectiveRenameMode === "custom" ? customName.trim() : null;
    exporting = true;
    progress = null;
    lastResult = null;
    localError = null;
    const unlisten = await listen<ExportProgress>("export://progress", event => {
      progress = event.payload;
    });
    try {
      const result = await exportSelectedImages(
        selection,
        target,
        effectiveRenameMode,
        custom,
        stripMetadata,
      );
      lastResult = result;
      const missing = result.missing > 0
        ? `，${formatCount(result.missing)} 张源文件不可用`
        : "";
      setNotice({
        tone: "success",
        text: `已导出 ${formatCount(result.exported)} 张图片${missing}。`,
      });
      await requestSelection();
    } catch (cause) {
      localError = errorText(cause);
    } finally {
      unlisten();
      progress = null;
      exporting = false;
    }
  }

  function folderName(path: string): string {
    return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
  }
</script>

<div class="export-page">
  <section class="selection-card" class:is-ready={selectedCount > 0}>
    <span class="step">1</span>
    <div class="card-copy">
      <h3>在主窗口选择图片</h3>
      {#if selectedCount > 0}
        <p>已读取主窗口当前选中的 <strong>{formatCount(selectedCount)}</strong> 张图片。</p>
      {:else}
        <p>尚未选中图片。请返回主窗口勾选需要导出的图片。</p>
      {/if}
    </div>
    <button type="button" class="btn" onclick={() => void returnToMain()}>
      返回主窗口选择
    </button>
  </section>

  <section class="option-card">
    <div class="option-heading">
      <span class="step">2</span>
      <div>
        <h3>选择导出位置 <em>必选</em></h3>
        <p>会在所选位置内创建独立的“智能表格图片导出”文件夹，不覆盖已有文件。</p>
      </div>
    </div>
    <button
      type="button"
      class="folder-picker"
      class:has-value={Boolean(destination)}
      onclick={() => void chooseDestination()}
    >
      <span class="folder-icon" aria-hidden="true">▱</span>
      <span class="folder-copy">
        <strong>{destination ? folderName(destination) : "选择导出文件夹…"}</strong>
        <small title={destination ?? undefined}>{destination ?? "尚未选择"}</small>
      </span>
      <span class="change-label">{destination ? "更换" : "选择"}</span>
    </button>
  </section>

  <section class="option-card">
    <div class="option-heading">
      <span class="step">3</span>
      <div>
        <h3>文件名</h3>
        <p>默认保留原文件名；同名文件会自动追加序号。</p>
      </div>
      <label class="switch-row">
        <input type="checkbox" bind:checked={renameEnabled} />
        <span>重命名</span>
      </label>
    </div>

    {#if renameEnabled}
      <div class="rename-options">
        <label class:is-active={renameMode === "random"}>
          <input type="radio" bind:group={renameMode} value="random" />
          <span>
            <strong>随机乱码</strong>
            <small>例如 a4f083bd7c19e260.png</small>
          </span>
        </label>
        <label class:is-active={renameMode === "custom"}>
          <input type="radio" bind:group={renameMode} value="custom" />
          <span>
            <strong>自定义命名</strong>
            <small>按“名称_1、名称_2…”顺序生成</small>
          </span>
        </label>
      </div>
      {#if renameMode === "custom"}
        <label class="custom-name">
          <span>文件名前缀</span>
          <div>
            <input
              type="text"
              bind:value={customName}
              maxlength="120"
              placeholder="例如：胡桃精选"
            />
            <code>{customName.trim() || "自定义名称"}_1.png</code>
          </div>
        </label>
      {/if}
    {/if}
  </section>

  <section class="option-card metadata-card">
    <div class="option-heading">
      <span class="step">4</span>
      <div>
        <h3>图片元数据</h3>
        <p>需要时重新编码导出副本的像素，彻底移除提示词、参数和其他元数据。</p>
      </div>
      <label class="switch-row danger-switch">
        <input type="checkbox" bind:checked={stripMetadata} />
        <span>抹除元数据</span>
      </label>
    </div>
    {#if stripMetadata}
      <p class="safety-note">
        只处理新导出的图片副本；资料库记录、原图和受管原件都不会被修改。
      </p>
    {/if}
  </section>

  {#if localError}
    <p class="message error" role="alert">{localError}</p>
  {/if}

  {#if exporting && progress}
    <div class="progress-card">
      <div>
        <strong>正在导出图片…</strong>
        <span>{formatCount(progress.processed)} / {formatCount(progress.total)}</span>
      </div>
      <progress value={progress.processed} max={Math.max(1, progress.total)}></progress>
    </div>
  {/if}

  {#if lastResult}
    <div class="result-card">
      <strong>导出完成</strong>
      <span>
        已导出 {formatCount(lastResult.exported)} 张
        {lastResult.missing > 0 ? `，${formatCount(lastResult.missing)} 张源文件不可用` : ""}
      </span>
      <code title={lastResult.directory}>{lastResult.directory}</code>
    </div>
  {/if}

  <footer class="action-bar">
    <div>
      {#if selectedCount === 0}
        <span>请先在主窗口选择图片</span>
      {:else if !destination}
        <span>请选择导出文件夹</span>
      {:else if !customNameValid}
        <span>请输入自定义文件名前缀</span>
      {:else}
        <span>准备导出 {formatCount(selectedCount)} 张图片</span>
      {/if}
    </div>
    <button
      type="button"
      class="btn btn-primary export-button"
      disabled={!canExport}
      onclick={() => void runExport()}
    >
      {exporting ? "正在导出…" : "开始导出"}
    </button>
  </footer>
</div>

<style>
  .export-page {
    min-height: 100%;
    padding: 24px 28px 32px;
  }

  .selection-card,
  .option-card {
    margin-bottom: 14px;
    padding: 18px 20px;
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    background: var(--surface);
    box-shadow: var(--shadow-1);
  }

  .selection-card {
    display: grid;
    grid-template-columns: 34px minmax(0, 1fr) auto;
    align-items: center;
    gap: 14px;
  }

  .selection-card.is-ready {
    border-color: color-mix(in srgb, var(--success) 35%, var(--border));
    background: color-mix(in srgb, var(--success-soft) 28%, var(--surface));
  }

  .step {
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    flex: none;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: var(--font-sm);
    font-weight: 700;
  }

  .card-copy,
  .option-heading > div {
    min-width: 0;
  }

  h3 {
    font-size: var(--font-lg);
  }

  h3 em {
    margin-left: 6px;
    color: var(--danger);
    font-size: var(--font-xs);
    font-style: normal;
    font-weight: 600;
  }

  .card-copy p,
  .option-heading p {
    margin-top: 4px;
    color: var(--text-2);
    font-size: var(--font-sm);
  }

  .option-heading {
    display: grid;
    grid-template-columns: 34px minmax(0, 1fr) auto;
    align-items: center;
    gap: 14px;
  }

  .folder-picker {
    width: calc(100% - 48px);
    min-height: 62px;
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 16px 0 0 48px;
    padding: 10px 13px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-s);
    background: var(--surface-2);
    color: var(--text-2);
    text-align: left;
    transition: border-color 120ms ease, background 120ms ease;
  }

  .folder-picker:hover {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .folder-picker.has-value {
    border-style: solid;
  }

  .folder-icon {
    font-size: 24px;
    color: var(--accent);
  }

  .folder-copy {
    min-width: 0;
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
  }

  .folder-copy strong,
  .folder-copy small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .folder-copy strong {
    color: var(--text-1);
    font-size: var(--font-md);
  }

  .folder-copy small,
  .change-label {
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .switch-row {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--text-2);
    font-size: var(--font-sm);
    font-weight: 600;
    white-space: nowrap;
  }

  .switch-row input {
    width: 17px;
    height: 17px;
    accent-color: var(--accent);
  }

  .danger-switch input {
    accent-color: var(--danger);
  }

  .rename-options {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
    margin: 16px 0 0 48px;
  }

  .rename-options label {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--surface-2);
  }

  .rename-options label.is-active {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .rename-options input {
    margin-top: 3px;
    accent-color: var(--accent);
  }

  .rename-options span {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .rename-options strong {
    font-size: var(--font-sm);
  }

  .rename-options small {
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .custom-name {
    display: grid;
    grid-template-columns: 112px minmax(0, 1fr);
    align-items: center;
    gap: 12px;
    margin: 14px 0 0 48px;
    color: var(--text-2);
    font-size: var(--font-sm);
  }

  .custom-name > div {
    display: grid;
    grid-template-columns: minmax(140px, 1fr) minmax(120px, auto);
    align-items: center;
    gap: 10px;
  }

  .custom-name input {
    min-width: 0;
    height: 36px;
    padding: 0 11px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    background: var(--surface);
    color: var(--text-1);
  }

  .custom-name code {
    overflow: hidden;
    color: var(--text-3);
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: var(--font-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .safety-note {
    margin: 14px 0 0 48px;
    padding: 10px 12px;
    border-radius: var(--radius-s);
    background: color-mix(in srgb, var(--danger-soft) 55%, var(--surface));
    color: var(--text-2);
    font-size: var(--font-sm);
  }

  .message,
  .progress-card,
  .result-card {
    margin-top: 14px;
    padding: 12px 14px;
    border-radius: var(--radius-s);
  }

  .message.error {
    background: var(--danger-soft);
    color: var(--danger);
    font-size: var(--font-sm);
  }

  .progress-card,
  .result-card {
    border: 1px solid var(--border);
    background: var(--surface);
  }

  .progress-card > div,
  .result-card {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .progress-card > div {
    flex-direction: row;
    justify-content: space-between;
    font-size: var(--font-sm);
  }

  .progress-card progress {
    width: 100%;
    height: 6px;
    margin-top: 10px;
    accent-color: var(--accent);
  }

  .result-card span,
  .result-card code {
    color: var(--text-2);
    font-size: var(--font-sm);
  }

  .result-card code {
    overflow: hidden;
    font-family: var(--font);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .action-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-top: 18px;
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    background: color-mix(in srgb, var(--surface) 92%, transparent);
    box-shadow: var(--shadow-2);
    backdrop-filter: blur(10px);
  }

  .action-bar span {
    color: var(--text-2);
    font-size: var(--font-sm);
  }

  .export-button {
    min-width: 116px;
  }

  @media (max-width: 760px) {
    .export-page {
      padding: 18px 18px 24px;
    }

    .selection-card {
      grid-template-columns: 34px minmax(0, 1fr);
    }

    .selection-card .btn {
      grid-column: 2;
      justify-self: start;
    }

    .option-heading {
      grid-template-columns: 34px minmax(0, 1fr);
    }

    .option-heading .switch-row {
      grid-column: 2;
      justify-self: start;
    }

    .folder-picker,
    .rename-options,
    .custom-name,
    .safety-note {
      width: auto;
      margin-left: 0;
    }

    .rename-options {
      grid-template-columns: 1fr;
    }

    .custom-name,
    .custom-name > div {
      grid-template-columns: 1fr;
    }

  }
</style>
