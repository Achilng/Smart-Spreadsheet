<script lang="ts">
  import type { LucideIcon } from "@lucide/svelte";
  import Braces from "@lucide/svelte/icons/braces";
  import Database from "@lucide/svelte/icons/database";
  import FileOutput from "@lucide/svelte/icons/file-output";
  import ListFilter from "@lucide/svelte/icons/list-filter";
  import Search from "@lucide/svelte/icons/search";
  import ScanSearch from "@lucide/svelte/icons/scan-search";
  import Settings2 from "@lucide/svelte/icons/settings-2";
  import Shuffle from "@lucide/svelte/icons/shuffle";
  import Workflow from "@lucide/svelte/icons/workflow";
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  import { app, refreshSnapshot } from "../../stores/app-state.svelte";
  import { installCloseGuards, registerCloseGuard } from "../../stores/close-guard";
  import {
    clearHistory,
    history,
    redoLastAction,
    undoLastAction,
  } from "../../stores/history.svelte";
  import Notice from "../../ui/Notice.svelte";
  import WindowControls from "../../ui/WindowControls.svelte";
  import ArtistGeneratorView from "./ArtistGeneratorView.svelte";
  import ArtistPrefixTool from "./ArtistPrefixTool.svelte";
  import AutomationRulesTool from "./AutomationRulesTool.svelte";
  import DataManagementTool from "./DataManagementTool.svelte";
  import ImageExportTool from "./ImageExportTool.svelte";
  import ImageSearchTool from "./ImageSearchTool.svelte";
  import JsonDedupeView from "./JsonDedupeView.svelte";
  import LibraryMaintenanceTool from "./LibraryMaintenanceTool.svelte";
  import QuickEditTool from "./QuickEditTool.svelte";

  type ToolId =
    | "automationRules"
    | "quickEdit"
    | "artistPrefix"
    | "artist"
    | "imageSearch"
    | "imageExport"
    | "jsonDedupe"
    | "maintenance"
    | "data";

  interface ToolDefinition {
    id: ToolId;
    label: string;
    description: string;
    group: "常用工具" | "文件处理" | "资料库维护";
    requiresLibrary: boolean;
    icon: LucideIcon;
    /** 页面自带完整头部（如自动规则的 editor-head），外壳不再渲染 content-header */
    fullBleed?: boolean;
  }

  const tools: ToolDefinition[] = [
    {
      id: "automationRules",
      label: "自动规则",
      description: "编写导入后自动检查与整理规则",
      group: "常用工具",
      requiresLibrary: false,
      icon: Workflow,
      fullBleed: true,
    },
    {
      id: "quickEdit",
      label: "快速整理",
      description: "按提示词组合批量打 Tag 或分组",
      group: "常用工具",
      requiresLibrary: true,
      icon: ListFilter,
    },
    {
      id: "artistPrefix",
      label: "画师前缀修正",
      description: "根据库内已有 artist: 标注修正裸画师 Tag",
      group: "常用工具",
      requiresLibrary: true,
      icon: ScanSearch,
    },
    {
      id: "artist",
      label: "随机画师串",
      description: "从画师池随机生成 NovelAI 提示词",
      group: "常用工具",
      requiresLibrary: true,
      icon: Shuffle,
    },
    {
      id: "imageSearch",
      label: "以图搜图",
      description: "使用感知哈希查找库内相似图片",
      group: "常用工具",
      requiresLibrary: true,
      icon: Search,
    },
    {
      id: "imageExport",
      label: "导出工具",
      description: "导出主窗口选区或本地图片并按需清除元数据",
      group: "文件处理",
      // 纯本地文件的元数据清洗不依赖资料库，空库时也应可用
      requiresLibrary: false,
      icon: FileOutput,
    },
    {
      id: "jsonDedupe",
      label: "智绘姬 JSON 去重",
      description: "检查并清理重复预设",
      group: "文件处理",
      requiresLibrary: false,
      icon: Braces,
    },
    {
      id: "maintenance",
      label: "资料库维护",
      description: "感知哈希与失败图片目录",
      group: "资料库维护",
      requiresLibrary: true,
      icon: Settings2,
    },
    {
      id: "data",
      label: "数据管理",
      description: "迁移数据目录或重置资料库",
      group: "资料库维护",
      requiresLibrary: true,
      icon: Database,
    },
  ];

  const groups = ["常用工具", "文件处理", "资料库维护"] as const;

  let activeTool = $state<ToolId>("automationRules");
  const visited = $state<Record<ToolId, boolean>>({
    automationRules: false,
    quickEdit: false,
    artistPrefix: false,
    artist: false,
    imageSearch: false,
    imageExport: false,
    jsonDedupe: false,
    maintenance: false,
    data: false,
  });

  const hasLibrary = $derived(
    Boolean(
      app.snapshot?.dataDirectory &&
        !app.snapshot.startupError &&
        (app.snapshot.library?.rowCount ?? 0) > 0,
    ),
  );
  const activeDefinition = $derived(
    tools.find(tool => tool.id === activeTool) ?? tools[0],
  );

  onMount(() => {
    void refreshSnapshot();
    let disposed = false;
    let uninstallGuards: (() => void) | null = null;
    let unlistenFocus: (() => void) | null = null;
    let unlistenLibraryChange: (() => void) | null = null;
    // 关窗守卫：进行中的任务与撤回能力都会随窗口关闭而消失，先确认
    const unregisterGuard = registerCloseGuard(() => {
      if (app.busy || history.busy) return "还有后台任务正在进行";
      if (app.phashProgress) return "感知哈希刷新尚未完成";
      if (app.exportProgress) return "导出任务尚未完成";
      if (history.undoCount > 0) {
        return `关闭后将无法撤回本窗口的 ${history.undoCount} 步批量修改`;
      }
      return null;
    });
    void installCloseGuards().then(fn => {
      if (disposed) fn();
      else uninstallGuards = fn;
    });
    // 主窗口导入/删除后快照会过时——窗口重获焦点时刷新，
    // 保证“需要资料库”的工具可用性跟随主窗口实际状态。
    void getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => {
        if (focused && !app.busy) {
          void refreshSnapshot();
        }
      })
      .then(fn => {
        if (disposed) fn();
        else unlistenFocus = fn;
      });
    // 主窗口的资料库变更广播（导入完成/删除/编辑/撤销）也即时刷新。
    // origin=main 表示主窗口自己改了库：工具箱撤销栈里记录的 rowId/前后状态
    // 已不可信，必须清空，否则在这里撤回会把主窗口的手工修改一并抹掉。
    // origin=toolbox 是本窗口操作的回流通知，不能清自己刚记的撤销。
    void listen<string>("main://library-changed", event => {
      if (event.payload !== "toolbox" && history.undoCount + history.redoCount > 0) {
        clearHistory();
      }
      if (!app.busy) {
        void refreshSnapshot();
      }
    }).then(fn => {
      if (disposed) fn();
      else unlistenLibraryChange = fn;
    });
    return () => {
      disposed = true;
      unregisterGuard();
      uninstallGuards?.();
      unlistenFocus?.();
      unlistenLibraryChange?.();
    };
  });

  $effect(() => {
    if (!app.loaded) return;
    const selected = tools.find(tool => tool.id === activeTool);
    if (selected?.requiresLibrary && !hasLibrary) {
      activeTool = "jsonDedupe";
      visited.jsonDedupe = true;
    } else {
      visited[activeTool] = true;
    }
  });

  function selectTool(tool: ToolDefinition): void {
    if (tool.requiresLibrary && !hasLibrary) return;
    activeTool = tool.id;
    visited[tool.id] = true;
  }

  /**
   * 窗口级撤销/重做快捷键：撤销栈是工具箱全窗口共享的（快速整理、画师前缀
   * 修正都往里记），快捷键不能只挂在单个工具面板上——否则在别的面板按
   * Ctrl+Z 是死键，用户没有任何入口撤回刚做的全库修改。
   */
  function onWindowKeydown(event: KeyboardEvent): void {
    const target = event.target;
    const isTextEditing =
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      (target instanceof HTMLElement && target.isContentEditable);
    if (!(event.ctrlKey || event.metaKey) || event.altKey || isTextEditing) return;

    const key = event.key.toLocaleLowerCase();
    if (key === "z") {
      event.preventDefault();
      if (event.shiftKey) {
        void redoLastAction();
      } else {
        void undoLastAction();
      }
    } else if (key === "y" && !event.shiftKey) {
      event.preventDefault();
      void redoLastAction();
    }
  }
</script>

<svelte:window
  onkeydown={onWindowKeydown}
  oncontextmenu={event => {
    // 输入控件放行原生右键菜单（粘贴等），其余位置屏蔽
    const target = event.target;
    if (
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      (target instanceof HTMLElement && target.isContentEditable)
    ) {
      return;
    }
    event.preventDefault();
  }}
/>

<div class="toolbox">
  <header class="titlebar" data-tauri-drag-region>
    <div class="brand" data-tauri-drag-region>
      <span data-tauri-drag-region>工具箱</span>
      <small data-tauri-drag-region>智能表格</small>
    </div>
    <div class="titlebar-history">
      <button
        type="button"
        class="btn btn-ghost history-btn"
        disabled={history.undoCount === 0 || history.busy || app.busy}
        title={history.undoLabel ? `撤回：${history.undoLabel}（Ctrl+Z）` : "没有可撤回的操作"}
        onclick={() => void undoLastAction()}
      >↶ 撤回</button>
      <button
        type="button"
        class="btn btn-ghost history-btn"
        disabled={history.redoCount === 0 || history.busy || app.busy}
        title={history.redoLabel ? `重做：${history.redoLabel}（Ctrl+Y）` : "没有可重做的操作"}
        onclick={() => void redoLastAction()}
      >↷ 重做</button>
    </div>
    <WindowControls />
  </header>

  <div class="toolbox-body">
    <aside class="tool-nav">
      <nav aria-label="工具列表">
        {#each groups as group}
          <section class="nav-group">
            <h2>{group}</h2>
            {#each tools.filter(tool => tool.group === group) as tool (tool.id)}
              <button
                type="button"
                class:is-active={activeTool === tool.id}
                disabled={tool.requiresLibrary && !hasLibrary}
                title={tool.requiresLibrary && !hasLibrary ? "需要先在主窗口导入资料库" : tool.description}
                onclick={() => selectTool(tool)}
              >
                <span class="tool-icon" aria-hidden="true">
                  <tool.icon size={15} strokeWidth={1.7} />
                </span>
                <span class="tool-label">
                  <strong>{tool.label}</strong>
                  <small>{tool.description}</small>
                </span>
              </button>
            {/each}
          </section>
        {/each}
      </nav>

    </aside>

    <main class="tool-content">
      {#if !activeDefinition.fullBleed}
        <header class="content-header">
          <div>
            <h2>{activeDefinition.label}</h2>
            <p>{activeDefinition.description}</p>
          </div>
        </header>
      {/if}

      <div class="tool-stack">
        {#if !app.loaded}
          <div class="empty-state loading-state">
            <span class="spinner" aria-hidden="true"></span>
            正在读取应用状态…
          </div>
        {:else}
          {#if visited.quickEdit}
            <section class="tool-panel" class:is-active={activeTool === "quickEdit"}>
              <QuickEditTool />
            </section>
          {/if}
          {#if visited.automationRules}
            <section class="tool-panel rules-panel" class:is-active={activeTool === "automationRules"}>
              <AutomationRulesTool />
            </section>
          {/if}
          {#if visited.artistPrefix}
            <section class="tool-panel" class:is-active={activeTool === "artistPrefix"}>
              <ArtistPrefixTool />
            </section>
          {/if}
          {#if visited.artist}
            <section class="tool-panel" class:is-active={activeTool === "artist"}>
              <ArtistGeneratorView />
            </section>
          {/if}
          {#if visited.imageSearch}
            <section class="tool-panel" class:is-active={activeTool === "imageSearch"}>
              <ImageSearchTool active={activeTool === "imageSearch"} />
            </section>
          {/if}
          {#if visited.imageExport}
            <section class="tool-panel" class:is-active={activeTool === "imageExport"}>
              <ImageExportTool active={activeTool === "imageExport"} />
            </section>
          {/if}
          {#if visited.jsonDedupe}
            <section class="tool-panel" class:is-active={activeTool === "jsonDedupe"}>
              <JsonDedupeView />
            </section>
          {/if}
          {#if visited.maintenance}
            <section class="tool-panel" class:is-active={activeTool === "maintenance"}>
              <LibraryMaintenanceTool />
            </section>
          {/if}
          {#if visited.data}
            <section class="tool-panel" class:is-active={activeTool === "data"}>
              <DataManagementTool />
            </section>
          {/if}
        {/if}
      </div>
    </main>
  </div>
</div>

<Notice />

<style>
  .toolbox {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }

  .titlebar {
    height: 52px;
    display: flex;
    align-items: stretch;
    justify-content: space-between;
    flex: none;
    padding-left: 20px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }

  .brand {
    display: flex;
    align-items: baseline;
    align-self: center;
    gap: 7px;
    color: var(--text);
    font-size: var(--font-lg);
    font-weight: 700;
    letter-spacing: -0.01em;
    white-space: nowrap;
  }

  .brand small {
    font-size: var(--font-xs);
    font-weight: 400;
    color: var(--text-3);
    letter-spacing: 0;
  }

  .titlebar-history {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-left: auto;
    margin-right: 8px;
  }

  .history-btn {
    min-height: 30px;
    padding: 4px 12px;
    font-size: var(--font-sm);
    white-space: nowrap;
  }

  .history-btn:disabled {
    opacity: 0.4;
  }

  .toolbox-body {
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .tool-nav {
    width: 240px;
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 18px 12px 14px;
    background: var(--surface);
    border-right: 1px solid var(--border);
  }

  nav {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .nav-group + .nav-group {
    margin-top: 16px;
  }

  .nav-group h2 {
    padding: 0 10px 5px;
    color: var(--text-3);
    font-size: var(--font-xs);
    font-weight: 650;
    letter-spacing: var(--ls-caps);
  }

  .nav-group button {
    position: relative;
    width: 100%;
    min-height: 52px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border: 1px solid transparent;
    border-radius: var(--radius-s);
    background: transparent;
    text-align: left;
  }

  .nav-group button:hover:not(:disabled) {
    background: var(--surface-2);
  }

  .nav-group button.is-active {
    background: var(--surface-3);
  }

  .nav-group button.is-active .tool-label strong {
    font-weight: 700;
  }

  .tool-icon {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    flex: none;
    border-radius: var(--radius-s);
    background: var(--surface-3);
    color: var(--text-2);
    font-size: var(--font-sm);
    font-weight: 700;
  }

  button.is-active .tool-icon {
    background: var(--accent-soft);
    color: var(--accent);
  }

  .tool-label {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .tool-label strong {
    font-size: var(--font-md);
    font-weight: 600;
  }

  .tool-label small {
    overflow: hidden;
    color: var(--text-3);
    font-size: var(--font-xs);
    font-weight: 400;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tool-content {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .content-header {
    display: flex;
    align-items: center;
    flex: none;
    padding: 22px 28px 12px;
  }

  .content-header h2 {
    font-size: 24px;
    font-weight: 700;
    letter-spacing: -0.022em;
  }

  .content-header p {
    margin-top: 2px;
    color: var(--text-3);
    font-size: 12.5px;
  }

  .tool-stack {
    flex: 1;
    min-height: 0;
    position: relative;
  }

  .tool-panel {
    position: absolute;
    inset: 0;
    display: none;
    overflow: auto;
  }

  .tool-panel.is-active {
    display: block;
  }

  .tool-panel.rules-panel {
    overflow: hidden;
  }

  .loading-state {
    height: 100%;
    font-size: var(--font-md);
  }
</style>
