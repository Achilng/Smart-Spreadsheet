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
  import { onMount } from "svelte";

  import { app, refreshSnapshot } from "../../stores/app-state.svelte";
  import Notice from "../../ui/Notice.svelte";
  import WindowControls from "../../ui/WindowControls.svelte";
  import ArtistGeneratorView from "./ArtistGeneratorView.svelte";
  import ArtistPrefixTool from "./ArtistPrefixTool.svelte";
  import DataManagementTool from "./DataManagementTool.svelte";
  import ImageExportTool from "./ImageExportTool.svelte";
  import ImageSearchTool from "./ImageSearchTool.svelte";
  import JsonDedupeView from "./JsonDedupeView.svelte";
  import LibraryMaintenanceTool from "./LibraryMaintenanceTool.svelte";
  import QuickEditTool from "./QuickEditTool.svelte";

  type ToolId =
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
  }

  const tools: ToolDefinition[] = [
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
      description: "用 Danbooru 词典识别并修正裸画师 Tag",
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
      requiresLibrary: true,
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

  let activeTool = $state<ToolId>("quickEdit");
  const visited = $state<Record<ToolId, boolean>>({
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
</script>

<svelte:window oncontextmenu={event => event.preventDefault()} />

<div class="toolbox">
  <header class="titlebar" data-tauri-drag-region>
    <div class="brand" data-tauri-drag-region>
      <span data-tauri-drag-region>工具箱</span>
    </div>
    <WindowControls />
  </header>

  <div class="toolbox-body">
    <aside class="tool-nav">
      <div class="nav-intro">
        <h1>工具箱</h1>
      </div>

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
      <header class="content-header">
        <div>
          <h2>{activeDefinition.label}</h2>
          <p>{activeDefinition.description}</p>
        </div>
      </header>

      <div class="tool-stack">
        {#if !app.loaded}
          <div class="loading-state">正在读取应用状态…</div>
        {:else}
          {#if visited.quickEdit}
            <section class="tool-panel" class:is-active={activeTool === "quickEdit"}>
              <QuickEditTool active={activeTool === "quickEdit"} />
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
              <ImageSearchTool />
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
    height: 40px;
    display: flex;
    align-items: stretch;
    justify-content: space-between;
    flex: none;
    padding-left: 14px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }

  .brand {
    display: flex;
    align-items: center;
    color: var(--text-2);
    font-size: var(--font-sm);
    font-weight: 600;
    letter-spacing: 0.02em;
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
    padding: 22px 12px 14px;
    background: var(--surface);
    border-right: 1px solid var(--border);
  }

  .nav-intro {
    padding: 0 10px 18px;
  }

  .nav-intro h1 {
    font-size: 22px;
    line-height: 1.25;
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
    font-weight: 600;
    letter-spacing: 0.08em;
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
    border-color: transparent;
    background: transparent;
    color: var(--accent);
  }

  .nav-group button.is-active::before {
    content: "";
    position: absolute;
    top: 10px;
    bottom: 10px;
    left: 1px;
    width: 3px;
    border-radius: var(--radius-full);
    background: var(--accent);
  }

  .tool-icon {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    flex: none;
    border-radius: 8px;
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
    min-height: 82px;
    display: flex;
    align-items: center;
    flex: none;
    padding: 18px 28px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }

  .content-header h2 {
    font-size: var(--font-xl);
    font-weight: 650;
  }

  .content-header p {
    margin-top: 2px;
    color: var(--text-3);
    font-size: var(--font-md);
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

  .loading-state {
    height: 100%;
    display: grid;
    place-items: center;
    color: var(--text-3);
    font-size: var(--font-md);
  }
</style>
