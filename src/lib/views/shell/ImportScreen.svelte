<script lang="ts">
  import { app, chooseDirectory, errorText, setNotice } from "../../stores/app-state.svelte";
  import {
    chooseImageArchive,
    chooseImageFolder,
    updateAutoArtistPrefixOnImport,
  } from "../../stores/import-actions.svelte";
  import { dropState, listenDragDrop } from "../../stores/drop-import.svelte";
  import { onMount } from "svelte";
  import { softFade, softFly, softPop } from "../../ui/motion";
  import { openToolboxWindow } from "../../windows/toolbox";
  import DropConfirmDialog from "./DropConfirmDialog.svelte";

  // 首屏也要支持拖拽导入：新用户的第一反应就是把文件夹拖进来
  onMount(() => listenDragDrop());

  async function openToolbox(): Promise<void> {
    try {
      await openToolboxWindow();
    } catch (error) {
      setNotice({ tone: "error", text: `无法打开工具箱：${errorText(error)}` });
    }
  }
</script>

<div class="center-screen">
  <div class="flow-card" in:softFly={{ duration: 230, y: 8 }}>
    <h2>导入第一批数据</h2>
    <p class="muted">
      资料库为追加式：可多次导入，已入库的图片自动跳过。支持 NovelAI PNG
      文件夹、单张 PNG，以及 zip/7z/rar 压缩包。无 metadata
      图片不会入库，会自动移到异常图片目录。
    </p>
    <p class="directory" title={app.snapshot?.dataDirectory}>
      数据目录：{app.snapshot?.dataDirectory}
    </p>
    <label class="import-setting">
      <input
        type="checkbox"
        role="switch"
        checked={app.snapshot?.autoArtistPrefixOnImport ?? false}
        disabled={app.busy}
        onchange={event => void updateAutoArtistPrefixOnImport(
          (event.currentTarget as HTMLInputElement).checked,
        )}
      />
      <span>
        <strong>导入时自动补全画师前缀</strong>
        <small>仅根据资料库中已有的明确 <code>artist:</code> 标注判断；导入时会提示，但不需要再次确认。</small>
      </span>
    </label>
    <div class="flow-actions">
      <button
        type="button"
        class="btn btn-primary"
        disabled={app.busy}
        onclick={() => void chooseImageFolder()}
      >
        导入图片文件夹
      </button>
      <button
        type="button"
        class="btn"
        disabled={app.busy}
        onclick={() => void openToolbox()}
      >
        编写自动规则
      </button>
      <button
        type="button"
        class="btn"
        disabled={app.busy}
        onclick={() => void chooseImageArchive()}
      >
        导入压缩包
      </button>
    </div>
    <p class="drop-tip">也可以直接把图片文件夹或压缩包拖到这个窗口里。</p>
    <p class="flow-hint">
      从旧版升级？
      <button
        type="button"
        class="link-btn"
        disabled={app.busy}
        onclick={() => void chooseDirectory("open")}
      >打开已有数据目录</button>
    </p>
  </div>
</div>

<DropConfirmDialog />

{#if dropState.dragging}
  <div class="drop-overlay" transition:softFade={{ duration: 120 }}>
    <div class="drop-hint" transition:softPop={{ duration: 150, y: 4, start: 0.98 }}>
      松开鼠标以导入图片
    </div>
  </div>
{/if}

<style>
  .directory {
    font-size: var(--font-sm);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .drop-tip {
    margin-top: 10px;
    font-size: var(--font-sm);
    color: var(--text-3);
  }

  .import-setting {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    margin: 14px 0;
    padding: 11px 13px;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--surface-2);
    cursor: pointer;
  }

  .import-setting input {
    margin-top: 2px;
  }

  .import-setting span {
    display: grid;
    gap: 3px;
  }

  .import-setting strong {
    color: var(--text);
    font-size: var(--font-sm);
  }

  .import-setting small {
    color: var(--text-3);
    font-size: var(--font-xs);
    line-height: 1.5;
  }

  .import-setting code {
    color: var(--accent);
  }

  .link-btn {
    background: none;
    border: none;
    color: var(--accent);
    cursor: pointer;
    font-size: var(--font-sm);
    padding: 0;
    text-decoration: underline;
  }

  .link-btn:hover {
    color: var(--accent-hover, var(--accent));
  }

  .link-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .drop-overlay {
    position: fixed;
    inset: 0;
    z-index: var(--z-dragdrop);
    display: grid;
    place-items: center;
    background: var(--overlay);
    pointer-events: none;
  }

  .drop-hint {
    padding: 20px 40px;
    background: var(--surface);
    border: 2px dashed var(--accent);
    border-radius: var(--radius-m);
    font-size: var(--font-xl);
    font-weight: 600;
    color: var(--accent);
  }
</style>
