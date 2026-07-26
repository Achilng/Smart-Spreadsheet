<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  import {
    app,
    bumpDataVersion,
    refreshSnapshot,
    resetAndReconfigure,
    setNotice,
    type MainStateChange,
  } from "./lib/stores/app-state.svelte";
  import { installCloseGuards, registerCloseGuard } from "./lib/stores/close-guard";
  import { clearHistory } from "./lib/stores/history.svelte";
  import ImportScreen from "./lib/views/shell/ImportScreen.svelte";
  import Notice from "./lib/ui/Notice.svelte";
  import WindowControls from "./lib/ui/WindowControls.svelte";
  import Workspace from "./lib/views/shell/Workspace.svelte";

  void refreshSnapshot();

  onMount(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    let uninstallGuards: (() => void) | null = null;

    // 长任务进行中关窗会拦截确认，避免导入/导出被拦腰截断
    const unregisterGuard = registerCloseGuard(() => {
      if (app.importProgress) return "图片导入尚未完成，关闭会中断导入";
      if (app.exportProgress) return "导出任务尚未完成";
      if (app.hashProgress) return "内容哈希补算尚未完成";
      if (app.phashProgress) return "感知哈希刷新尚未完成";
      if (app.busy) return "还有后台任务正在进行";
      return null;
    });
    void installCloseGuards().then(fn => {
      if (disposed) fn();
      else uninstallGuards = fn;
    });

    void listen<MainStateChange>("toolbox://app-state-changed", event => {
      if (event.payload === "libraryEdited") {
        clearHistory();
        bumpDataVersion({ preserveScroll: true });
        return;
      }
      void refreshSnapshot().then(() => {
        bumpDataVersion();
        setNotice({
          tone: "success",
          text: event.payload === "reset"
            ? "表格已重置，请重新导入数据。"
            : "数据目录已迁移，主窗口已重新连接。",
        });
      });
    }).then(fn => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
      unregisterGuard();
      uninstallGuards?.();
    };
  });

  const inWorkspace = $derived(
    Boolean(
      app.loaded &&
        app.snapshot &&
        !app.snapshot.startupError &&
        app.snapshot.dataDirectory &&
        (app.snapshot.library?.rowCount ?? 0) > 0,
    ),
  );

  function suppressDefaultContextMenu(event: MouseEvent): void {
    event.preventDefault();
  }
</script>

<svelte:window oncontextmenu={suppressDefaultContextMenu} />

{#if inWorkspace}
  <Workspace />
{:else}
  <!-- 无系统边框：流程页用独立标题条承担拖拽和窗口控制 -->
  <div class="flow-titlebar" data-tauri-drag-region>
    <span class="flow-app-name" data-tauri-drag-region>智能表格</span>
    <WindowControls />
  </div>
  <div class="flow-body">
    {#if !app.loaded}
      <div class="center-screen">
        <p class="muted">正在读取应用状态…</p>
      </div>
    {:else if app.snapshot?.startupError}
      <div class="center-screen">
        <div class="flow-card">
          <h2>无法打开已配置的数据目录</h2>
          <p class="muted">{app.snapshot.startupError}</p>
          <div class="flow-actions">
            <button
              type="button"
              class="btn btn-primary"
              disabled={app.busy}
              onclick={() => void resetAndReconfigure()}
            >
              重新配置
            </button>
          </div>
          <p class="flow-hint">重新配置将清除当前定位信息，回到初始设置页面。</p>
        </div>
      </div>
    {:else}
      <ImportScreen />
    {/if}
  </div>
{/if}

<Notice />

<style>
  .flow-titlebar {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    height: 52px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-left: 20px;
    z-index: var(--z-nav);
  }

  .flow-app-name {
    font-size: var(--font-sm);
    font-weight: 600;
    color: var(--text-3);
    letter-spacing: 0.04em;
  }

  .flow-body {
    height: 100%;
  }
</style>
