<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import { invoke } from "@tauri-apps/api/core";

  import { app, dismissNotice, formatCount, setNotice } from "../stores/app-state.svelte";
  import { flipDuration, softFly } from "./motion";
  import { flip } from "svelte/animate";

  let cancelling = $state(false);

  /** 导入与“更新现有图片”管线均支持取消；其余进度不显示取消按钮。 */
  const cancellable = $derived(Boolean(app.importProgress));

  async function cancelTask(): Promise<void> {
    if (cancelling) return;
    cancelling = true;
    try {
      await invoke("cancel_current_task");
    } catch {
      setNotice({ tone: "error", text: "取消请求发送失败。" });
    }
  }

  // 进度条消失（任务结束）后复位取消按钮状态
  $effect(() => {
    if (!app.importProgress) {
      cancelling = false;
    }
  });

  const progressText = $derived.by(() => {
    const hashing = app.hashProgress;
    if (hashing) {
      const unreadable = hashing.unreadable > 0
        ? `，${formatCount(hashing.unreadable)} 行图片不可读`
        : "";
      return `正在升级图片指纹 ${formatCount(hashing.processed)} / ${formatCount(hashing.total)}${unreadable}`;
    }
    const progress = app.importProgress;
    if (progress) {
      switch (progress.stage) {
        case "extracting":
          return "正在解压压缩包…";
        case "scanning":
          return progress.total > 0
            ? `已发现 ${formatCount(progress.total)} 张 PNG`
            : "正在扫描 PNG…";
        case "hashing":
          return `正在检查图片内容 ${formatCount(progress.processed)} / ${formatCount(progress.total)}`;
        case "processing":
          return `正在读取元数据 ${formatCount(progress.processed)} / ${formatCount(progress.total)}`;
        case "perceptualHashing":
          return `正在计算感知哈希 ${formatCount(progress.processed)} / ${formatCount(progress.total)}`;
        case "copying":
          return `正在复制图片进资料库 ${formatCount(progress.processed)} / ${formatCount(progress.total)}`;
        default:
          return null;
      }
    }
    const phash = app.phashProgress;
    if (phash) {
      const unreadable = phash.unreadable > 0
        ? `，${formatCount(phash.unreadable)} 张不可读`
        : "";
      return `正在计算感知哈希 ${formatCount(phash.processed)} / ${formatCount(phash.total)}${unreadable}`;
    }
    const vibe = app.vibeBackfillProgress;
    if (vibe) {
      return `正在建立 VIBE 聚合索引 ${formatCount(vibe.processed)} / ${formatCount(vibe.total)}（升级后一次性工作，可继续正常使用）`;
    }
    const exporting = app.exportProgress;
    if (exporting) {
      return `正在导出 ${formatCount(exporting.processed)} / ${formatCount(exporting.total)}`;
    }
    return null;
  });
  const progressPercent = $derived.by(() => {
    const hashing = app.hashProgress;
    if (hashing && hashing.total > 0) {
      return Math.round((hashing.processed / hashing.total) * 100);
    }
    const importing = app.importProgress;
    if (importing) {
      if (
        (importing.stage !== "hashing" && importing.stage !== "processing" && importing.stage !== "perceptualHashing" && importing.stage !== "copying") ||
        importing.total === 0
      ) {
        return null;
      }
      return Math.round((importing.processed / importing.total) * 100);
    }
    const phash = app.phashProgress;
    if (phash && phash.total > 0) {
      return Math.round((phash.processed / phash.total) * 100);
    }
    const vibe = app.vibeBackfillProgress;
    if (vibe && vibe.total > 0) {
      return Math.round((vibe.processed / vibe.total) * 100);
    }
    const exporting = app.exportProgress;
    if (exporting && exporting.total > 0) {
      return Math.round((exporting.processed / exporting.total) * 100);
    }
    return null;
  });
</script>

<!-- 通知与进度分栏共存：右下角纵向堆叠，不压住底部选择条 -->
<div class="toast-stack">
  {#each app.notices as notice (notice.id)}
    <div
      class="toast toast-{notice.tone}"
      role={notice.tone === "error" ? "alert" : "status"}
      animate:flip={{ duration: flipDuration(170) }}
      transition:softFly={{ duration: 180, y: 8 }}
    >
      <span>{notice.text}</span>
      <button type="button" aria-label="关闭提示" onclick={() => dismissNotice(notice.id)}><X size={14} strokeWidth={2} /></button>
    </div>
  {/each}
  {#if progressText}
    <div class="toast toast-progress" role="status" transition:softFly={{ duration: 180, y: 8 }}>
      <span class="progress-copy">
        <span>{progressText}</span>
        {#if app.autoArtistPrefixImportActive}
          <small>提示：导入完成后将自动补全有库内证据的画师前缀，无需确认。</small>
        {/if}
      </span>
      {#if progressPercent != null}
        <span class="progress-track" aria-hidden="true">
          <span class="progress-fill" style:transform="scaleX({progressPercent / 100})"></span>
        </span>
      {/if}
      {#if cancellable}
        <button
          type="button"
          class="cancel-btn"
          disabled={cancelling}
          onclick={() => void cancelTask()}
        >{cancelling ? "正在取消…" : "取消"}</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .toast-stack {
    position: fixed;
    right: 16px;
    /* 底部选择条高 52px；抬高到它之上，两者互不遮挡 */
    bottom: 64px;
    z-index: var(--z-toast);
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 8px;
    pointer-events: none;
  }

  .toast {
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: min(560px, calc(100vw - 48px));
    padding: 10px 14px;
    border-radius: var(--radius-s);
    border: 1px solid;
    background: var(--surface);
    box-shadow: var(--shadow-2);
    font-size: var(--font-md);
    pointer-events: auto;
  }

  .toast-success {
    border-color: var(--success);
    color: var(--success);
    background: var(--success-soft);
  }

  .toast-error {
    border-color: var(--danger);
    color: var(--danger);
    background: var(--danger-soft);
  }

  .toast-progress {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }

  .progress-track {
    width: 120px;
    height: 6px;
    border-radius: var(--radius-full);
    background: var(--surface);
    overflow: hidden;
    flex: none;
  }

  .progress-copy {
    display: grid;
    gap: 2px;
  }

  .progress-copy small {
    color: var(--text-2);
    font-size: var(--font-xs);
  }

  .progress-fill {
    display: block;
    width: 100%;
    height: 100%;
    border-radius: var(--radius-full);
    background: var(--accent);
    transform-origin: left;
    transition: transform var(--motion-fast) linear;
  }

  .toast span {
    overflow-wrap: anywhere;
  }

  .toast button {
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: none;
    color: inherit;
    line-height: 1;
    padding: 2px;
    transition: transform var(--motion-press) var(--ease-responsive);
  }

  .toast button:active {
    transform: translateY(1px) scale(0.9);
  }

  .cancel-btn {
    flex: none;
    padding: 3px 10px;
    border: 1px solid var(--accent);
    border-radius: var(--radius-full);
    background: var(--surface);
    color: var(--accent);
    font-size: var(--font-xs);
    cursor: pointer;
  }

  .cancel-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }
</style>
