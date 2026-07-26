<script lang="ts">
  import X from "@lucide/svelte/icons/x";

  import { app, formatCount, setNotice } from "../stores/app-state.svelte";
  import { softFly } from "./motion";

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
        (importing.stage !== "hashing" && importing.stage !== "processing" && importing.stage !== "perceptualHashing") ||
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
    const exporting = app.exportProgress;
    if (exporting && exporting.total > 0) {
      return Math.round((exporting.processed / exporting.total) * 100);
    }
    return null;
  });
</script>

{#if progressText}
  <div class="toast toast-progress" role="status" transition:softFly={{ duration: 180, y: 8 }}>
    <span>{progressText}</span>
    {#if progressPercent != null}
      <span class="progress-track" aria-hidden="true">
        <span class="progress-fill" style:transform="scaleX({progressPercent / 100})"></span>
      </span>
    {/if}
  </div>
{:else if app.notice}
  <div class="toast toast-{app.notice.tone}" role="status" transition:softFly={{ duration: 180, y: 8 }}>
    <span>{app.notice.text}</span>
    <button type="button" aria-label="关闭提示" onclick={() => setNotice(null)}><X size={14} strokeWidth={2} /></button>
  </div>
{/if}

<style>
  .toast {
    position: fixed;
    bottom: 20px;
    left: 50%;
    transform: translateX(-50%);
    z-index: var(--z-toast);
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: min(640px, calc(100vw - 48px));
    padding: 10px 14px;
    border-radius: var(--radius-s);
    border: 1px solid;
    background: var(--surface);
    box-shadow: var(--shadow-2);
    font-size: var(--font-md);
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
</style>
