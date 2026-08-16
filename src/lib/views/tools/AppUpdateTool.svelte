<script lang="ts">
  import CheckCircle2 from "@lucide/svelte/icons/check-circle-2";
  import CircleArrowUp from "@lucide/svelte/icons/circle-arrow-up";
  import Download from "@lucide/svelte/icons/download";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import { getVersion } from "@tauri-apps/api/app";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
  import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
  import { onMount } from "svelte";

  import { app, errorText, setNotice } from "../../stores/app-state.svelte";
  import { history } from "../../stores/history.svelte";

  type UpdateStatus = "idle" | "checking" | "latest" | "available" | "downloading" | "installing" | "error";

  let currentVersion = $state("读取中…");
  let status = $state<UpdateStatus>("idle");
  let pendingUpdate = $state<Update | null>(null);
  let lastCheckedAt = $state<Date | null>(null);
  let errorMessage = $state("");
  let downloadedBytes = $state(0);
  let totalBytes = $state<number | null>(null);

  const working = $derived(
    status === "checking" || status === "downloading" || status === "installing",
  );
  const progressRatio = $derived(
    totalBytes && totalBytes > 0 ? Math.min(downloadedBytes / totalBytes, 1) : 0,
  );
  const installBlockedReason = $derived.by(() => {
    if (status !== "available") return null;
    if (
      app.busy ||
      history.busy ||
      app.importProgress ||
      app.exportProgress ||
      app.phashProgress ||
      app.hashProgress ||
      app.vibeBackfillProgress ||
      app.migrationProgress
    ) {
      return "还有任务正在进行，请等待任务结束后再更新。";
    }
    if (history.undoCount + history.redoCount > 0) {
      return "工具箱仍有可撤回或重做的修改。请关闭并重新打开工具箱，确认放弃这些记录后再更新。";
    }
    return null;
  });

  onMount(() => {
    void getVersion()
      .then(version => {
        currentVersion = version;
      })
      .catch(error => {
        currentVersion = "未知";
        setNotice({ tone: "error", text: `无法读取当前版本：${errorText(error)}` });
      });

    return () => {
      if (pendingUpdate) {
        void pendingUpdate.close();
      }
    };
  });

  async function checkForUpdate(): Promise<void> {
    if (working) return;
    status = "checking";
    errorMessage = "";
    downloadedBytes = 0;
    totalBytes = null;

    try {
      if (pendingUpdate) {
        await pendingUpdate.close();
        pendingUpdate = null;
      }
      const update = await check({ timeout: 15_000 });
      lastCheckedAt = new Date();
      if (update) {
        pendingUpdate = update;
        status = "available";
      } else {
        status = "latest";
      }
    } catch (error) {
      errorMessage = errorText(error);
      status = "error";
    }
  }

  async function installUpdate(): Promise<void> {
    if (!pendingUpdate || working) return;
    if (installBlockedReason) {
      setNotice({ tone: "error", text: installBlockedReason });
      return;
    }

    const confirmed = await confirmDialog(
      `将下载并安装智能表格 ${pendingUpdate.version}。安装时主窗口和工具箱会关闭，完成后自动重新打开。是否继续？`,
      {
        title: "安装应用更新",
        kind: "warning",
        okLabel: "下载并安装",
        cancelLabel: "取消",
      },
    );
    if (!confirmed) return;

    status = "downloading";
    downloadedBytes = 0;
    totalBytes = null;
    app.busy = true;
    setNotice(null);

    try {
      await pendingUpdate.download(handleDownloadEvent, { timeout: 120_000 });
      status = "installing";
      await pendingUpdate.install();
      await relaunch();
    } catch (error) {
      errorMessage = errorText(error);
      status = "error";
      setNotice({ tone: "error", text: `更新安装失败：${errorMessage}` });
    } finally {
      app.busy = false;
    }
  }

  function handleDownloadEvent(event: DownloadEvent): void {
    if (event.event === "Started") {
      totalBytes = event.data.contentLength ?? null;
      return;
    }
    if (event.event === "Progress") {
      downloadedBytes += event.data.chunkLength;
      return;
    }
    downloadedBytes = totalBytes ?? downloadedBytes;
  }

  function formatCheckedAt(value: Date): string {
    return value.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  }

  function formatReleaseDate(value?: string): string {
    if (!value) return "";
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return date.toLocaleString("zh-CN", {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  function formatBytes(value: number): string {
    if (value < 1024) return `${value} B`;
    const units = ["KB", "MB", "GB"];
    let size = value / 1024;
    let index = 0;
    while (size >= 1024 && index < units.length - 1) {
      size /= 1024;
      index += 1;
    }
    return `${size >= 10 ? size.toFixed(0) : size.toFixed(1)} ${units[index]}`;
  }

  /** GitHub Release 通常使用 Markdown；这里仅做安全的纯文本清理，不注入 HTML。 */
  function formatReleaseNotes(value?: string): string {
    if (!value?.trim()) return "这个版本没有附带更新说明。";
    return value
      .trim()
      .replace(/^#{1,6}\s+/gm, "")
      .replace(/^\s*[-*+]\s+/gm, "• ")
      .replace(/\*\*([^*]+)\*\*/g, "$1")
      .replace(/`([^`]+)`/g, "$1");
  }
</script>

<div class="tool-page">
  <section class="update-card tool-card">
    <div class="card-icon"><CircleArrowUp size={21} strokeWidth={1.7} /></div>
    <div class="card-copy">
      <span class="overline">当前版本</span>
      <h3>智能表格 {currentVersion}</h3>
      <p>仅在你点击按钮后连接 GitHub Release，不会在启动时自动联网。</p>
      {#if lastCheckedAt}
        <small>最近检查：{formatCheckedAt(lastCheckedAt)}</small>
      {/if}
    </div>
    <button
      type="button"
      class="btn"
      disabled={working}
      onclick={() => void checkForUpdate()}
    >
      {#if status === "checking"}
        <span class="spinner button-spinner" aria-hidden="true"></span>
        正在检查…
      {:else}
        <RefreshCw size={15} strokeWidth={1.8} />
        {status === "idle" ? "检查更新" : "重新检查"}
      {/if}
    </button>
  </section>

  {#if status === "latest"}
    <section class="result-card latest-card tool-card" role="status">
      <div class="card-icon success-icon"><CheckCircle2 size={21} strokeWidth={1.8} /></div>
      <div class="card-copy">
        <h3>已经是最新版本</h3>
        <p>GitHub Release 暂时没有比 {currentVersion} 更新的版本。</p>
      </div>
    </section>
  {:else if status === "available" && pendingUpdate}
    <section class="release-card tool-card" aria-live="polite">
      <div class="release-head">
        <div>
          <span class="overline">发现新版本</span>
          <h3>{pendingUpdate.version}</h3>
          {#if pendingUpdate.date}
            <small>发布于 {formatReleaseDate(pendingUpdate.date)}</small>
          {/if}
        </div>
        <button
          type="button"
          class="btn btn-primary"
          disabled={Boolean(installBlockedReason)}
          title={installBlockedReason ?? "下载、校验并安装这个版本"}
          onclick={() => void installUpdate()}
        >
          <Download size={15} strokeWidth={1.8} />
          下载并安装
        </button>
      </div>
      {#if installBlockedReason}
        <p class="warning-box" role="status">{installBlockedReason}</p>
      {/if}
      <div class="release-notes">
        <h4>更新说明</h4>
        <p>{formatReleaseNotes(pendingUpdate.body)}</p>
      </div>
    </section>
  {:else if status === "downloading" || status === "installing"}
    <section class="progress-card tool-card" aria-live="polite">
      <div class="progress-head">
        <div>
          <span class="overline">{status === "downloading" ? "正在下载" : "正在安装"}</span>
          <h3>{pendingUpdate?.version ?? "新版本"}</h3>
        </div>
        {#if status === "downloading"}
          <span class="tabular">
            {formatBytes(downloadedBytes)}{totalBytes ? ` / ${formatBytes(totalBytes)}` : ""}
          </span>
        {:else}
          <span>即将重新打开应用…</span>
        {/if}
      </div>
      <span class="progress" class:is-indeterminate={!totalBytes || status === "installing"} role="progressbar">
        <span class="progress-fill" style:transform={`scaleX(${status === "installing" ? 1 : progressRatio})`}></span>
      </span>
      <p>请不要关闭应用。Windows 安装程序接手后，当前窗口会自动退出。</p>
    </section>
  {:else if status === "error"}
    <section class="result-card error-card tool-card" role="alert">
      <div class="card-copy">
        <span class="overline">检查或安装失败</span>
        <h3>暂时无法完成更新</h3>
        <p>{errorMessage}</p>
        <small>请确认网络可访问 GitHub 后重试；这不会影响资料库和其它功能。</small>
      </div>
    </section>
  {/if}

  <p class="source-note">
    更新源：github.com/Achilng/Smart-Spreadsheet · 安装包会先验证更新签名，验证失败不会安装。
  </p>
</div>

<style>
  .tool-page {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 100%;
    padding: 24px 28px 32px;
  }

  .update-card,
  .result-card {
    display: grid;
    grid-template-columns: 42px minmax(0, 1fr) auto;
    align-items: center;
    gap: 16px;
    padding: 20px;
  }

  .card-icon {
    width: 42px;
    height: 42px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-m);
    background: var(--accent-soft);
    color: var(--accent);
  }

  .success-icon {
    background: var(--success-soft);
    color: var(--success);
  }

  .card-copy {
    min-width: 0;
  }

  h3 {
    font-size: var(--font-lg);
  }

  h4 {
    font-size: var(--font-md);
  }

  .card-copy p,
  .progress-card p {
    margin-top: 3px;
    color: var(--text-2);
    font-size: var(--font-md);
  }

  small {
    display: block;
    margin-top: 6px;
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .button-spinner {
    width: 13px;
    height: 13px;
    border-width: 1.5px;
  }

  .release-card,
  .progress-card {
    padding: 20px;
  }

  .release-head,
  .progress-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
  }

  .release-notes {
    margin-top: 18px;
    padding-top: 16px;
    border-top: 1px solid var(--border-faint);
  }

  .release-notes p {
    max-height: 280px;
    overflow-y: auto;
    margin-top: 7px;
    padding-right: 8px;
    color: var(--text-2);
    font-size: var(--font-md);
    line-height: 1.65;
    white-space: pre-wrap;
  }

  .warning-box {
    margin-top: 14px;
    padding: 10px 12px;
    border-radius: var(--radius-s);
    background: var(--warning-soft);
    color: var(--warning);
    font-size: var(--font-md);
  }

  .progress-card .progress {
    margin-top: 16px;
  }

  .progress-head > span {
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .is-indeterminate .progress-fill {
    width: 38%;
    animation: update-progress 1.1s ease-in-out infinite;
  }

  .error-card {
    grid-template-columns: minmax(0, 1fr);
    border-color: color-mix(in srgb, var(--danger) 25%, var(--surface));
  }

  .error-card .overline {
    color: var(--danger);
  }

  .source-note {
    padding: 0 4px;
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  @keyframes update-progress {
    from {
      transform: translateX(-110%);
    }
    to {
      transform: translateX(280%);
    }
  }

  @media (max-width: 760px) {
    .update-card,
    .result-card {
      grid-template-columns: 42px minmax(0, 1fr);
    }

    .update-card > button {
      grid-column: 2;
      justify-self: start;
    }

    .release-head,
    .progress-head {
      align-items: flex-start;
      flex-direction: column;
      gap: 12px;
    }
  }
</style>
