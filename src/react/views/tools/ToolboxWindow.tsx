import { ConfirmationDialog } from "../../ui/ConfirmationDialog";
import { lazy, Suspense, useEffect, useRef, useState, type ComponentType } from "react";
import { Tooltip } from "radix-ui";
import { Braces, CircleArrowUp, Database, FileOutput, ListFilter, Search, ScanSearch, Settings2, Shuffle, Workflow, type LucideIcon } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { installCloseGuards, registerCloseGuard } from "../../../lib/stores/close-guard";
import { useLibrary } from "../../state/library";
import { useTasks } from "../../state/tasks";
import { clearHistory, redoLastAction, undoLastAction, useHistory } from "../../state/history";
import { refreshToolboxSnapshot, useToolProgress } from "../../state/tools/library-actions";
import { notify } from "../../state/notices";
import { errorText } from "../../../lib/utils/format";
import { Button } from "../../ui/controls";
import { Notices } from "../../ui/Notices";
import { WindowControls } from "../../ui/WindowControls";
import ArtistPrefixTool from "./ArtistPrefixTool";
import ArtistGeneratorView from "./ArtistGeneratorView";
import DataManagementTool from "./DataManagementTool";
import LibraryMaintenanceTool from "./LibraryMaintenanceTool";
import AppUpdateTool from "./AppUpdateTool";
import JsonDedupeView from "./JsonDedupeView";
import ImageSearchTool from "./ImageSearchTool";
import ImageExportTool from "./ImageExportTool";
import "./toolbox.css";
const AutomationRulesTool = lazy(() => import("./AutomationRulesTool"));
const QuickEditTool = lazy(() => import("./QuickEditTool"));
type ToolId = "automationRules" | "quickEdit" | "artistPrefix" | "artist" | "imageSearch" | "imageExport" | "jsonDedupe" | "maintenance" | "data" | "update";
interface Tool { id: ToolId; label: string; description: string; group: string; requiresLibrary: boolean; icon: LucideIcon; component: ComponentType<{ active: boolean }>; fullBleed?: boolean }
const tools: Tool[] = [
  { id: "automationRules", label: "自动规则", description: "编写导入后自动检查与整理规则", group: "常用工具", requiresLibrary: false, icon: Workflow, component: AutomationRulesTool, fullBleed: true },
  { id: "quickEdit", label: "快速整理", description: "按提示词组合批量打 Tag 或分组", group: "常用工具", requiresLibrary: true, icon: ListFilter, component: QuickEditTool },
  { id: "artistPrefix", label: "画师前缀修正", description: "根据库内已有 artist: 标注修正裸画师 Tag", group: "常用工具", requiresLibrary: true, icon: ScanSearch, component: ArtistPrefixTool },
  { id: "artist", label: "随机画师串", description: "从画师池随机生成 NovelAI 提示词", group: "常用工具", requiresLibrary: true, icon: Shuffle, component: ArtistGeneratorView },
  { id: "imageSearch", label: "以图搜图", description: "使用感知哈希查找库内相似图片", group: "常用工具", requiresLibrary: true, icon: Search, component: ImageSearchTool },
  { id: "imageExport", label: "导出工具", description: "导出主窗口选区或本地图片并按需清除元数据", group: "文件处理", requiresLibrary: false, icon: FileOutput, component: ImageExportTool },
  { id: "jsonDedupe", label: "智绘姬 JSON 去重", description: "检查并清理重复预设", group: "文件处理", requiresLibrary: false, icon: Braces, component: JsonDedupeView },
  { id: "maintenance", label: "资料库维护", description: "感知哈希与失败图片目录", group: "资料库维护", requiresLibrary: true, icon: Settings2, component: LibraryMaintenanceTool },
  { id: "data", label: "数据管理", description: "迁移数据目录或重置资料库", group: "资料库维护", requiresLibrary: true, icon: Database, component: DataManagementTool },
  { id: "update", label: "应用更新", description: "从 GitHub Release 检查并安装新版本", group: "应用", requiresLibrary: false, icon: CircleArrowUp, component: AppUpdateTool },
];
const groups = ["常用工具", "文件处理", "资料库维护", "应用"];
const isEditing = (target: EventTarget | null) => target instanceof HTMLElement && (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target.isContentEditable);

export function ToolboxWindow() {
  const [active, setActive] = useState<ToolId>("automationRules"), [visited, setVisited] = useState<Set<ToolId>>(new Set());
  const library = useLibrary(), tasks = useTasks(), history = useHistory(), deferred = useRef(false), invalidateHistory = useRef(false);
  const hasLibrary = !!library.snapshot?.dataDirectory && !library.snapshot.startupError && (library.snapshot.library?.rowCount ?? 0) > 0;
  const definition = tools.find(tool => tool.id === active)!;
  useEffect(() => {
    void refreshToolboxSnapshot(); let disposed = false; const cleanups: (() => void)[] = [];
    const keep = (promise: Promise<() => void>) => void promise.then(fn => { if (disposed) fn(); else cleanups.push(fn); }).catch(error => { if (!disposed) notify(errorText(error), "error"); });
    const guard = registerCloseGuard(() => {
      if (useTasks.getState().busy || useHistory.getState().busy) return "还有后台任务正在进行";
      const progress = useToolProgress.getState(); if (progress.phash || progress.migration) return "资料库维护尚未完成";
      const count = useHistory.getState().undoCount; return count ? `关闭后将无法撤回本窗口的 ${count} 步批量修改` : null;
    });
    keep(installCloseGuards());
    keep(getCurrentWindow().onFocusChanged(({ payload }) => { if (payload) { if (useTasks.getState().busy) deferred.current = true; else void refreshToolboxSnapshot(); } }));
    keep(listen<string>("main://library-changed", event => {
      if (event.payload !== "toolbox") { clearHistory(); invalidateHistory.current = true; }
      if (useTasks.getState().busy) deferred.current = true; else { invalidateHistory.current = false; void refreshToolboxSnapshot(); }
    }));
    const keydown = (event: KeyboardEvent) => { if (event.defaultPrevented || document.querySelector('[role="dialog"]') || isEditing(event.target) || !(event.ctrlKey || event.metaKey) || event.altKey) return; const key = event.key.toLocaleLowerCase(); if (key === "z") { event.preventDefault(); void (event.shiftKey ? redoLastAction() : undoLastAction()); } else if (key === "y" && !event.shiftKey) { event.preventDefault(); void redoLastAction(); } };
    const contextmenu = (event: MouseEvent) => { if (!isEditing(event.target)) event.preventDefault(); };
    window.addEventListener("keydown", keydown); window.addEventListener("contextmenu", contextmenu);
    return () => { disposed = true; guard(); cleanups.forEach(fn => fn()); window.removeEventListener("keydown", keydown); window.removeEventListener("contextmenu", contextmenu); };
  }, []);
  useEffect(() => { if (!tasks.busy && deferred.current) { deferred.current = false; if (invalidateHistory.current) { clearHistory(); invalidateHistory.current = false; } void refreshToolboxSnapshot(); } }, [tasks.busy]);
  useEffect(() => { if (!library.loaded) return; const next = definition.requiresLibrary && !hasLibrary ? "jsonDedupe" : active; if (next !== active) setActive(next); setVisited(current => current.has(next) ? current : new Set([...current, next])); }, [library.loaded, active, definition.requiresLibrary, hasLibrary]);
  return <Tooltip.Provider delayDuration={350}><div className="rt-window"><header className="rt-titlebar" data-tauri-drag-region><div className="rt-brand" data-tauri-drag-region>工具箱 <small data-tauri-drag-region>智能表格</small></div><div className="rt-title-actions"><Button variant="ghost" disabled={!history.undoCount || history.busy || tasks.busy} title={history.undoLabel ? `撤回：${history.undoLabel}（Ctrl+Z）` : "没有可撤回的操作"} onClick={() => void undoLastAction()}>↶ 撤回</Button><Button variant="ghost" disabled={!history.redoCount || history.busy || tasks.busy} title={history.redoLabel ? `重做：${history.redoLabel}（Ctrl+Y）` : "没有可重做的操作"} onClick={() => void redoLastAction()}>↷ 重做</Button></div><WindowControls /></header><div className="rt-body"><aside className="rt-nav"><nav aria-label="工具列表">{groups.map(group => <section key={group}><h2>{group}</h2>{tools.filter(tool => tool.group === group).map(tool => <button key={tool.id} className={active === tool.id ? "is-active" : undefined} aria-current={active === tool.id ? "page" : undefined} disabled={tool.requiresLibrary && !hasLibrary} title={tool.requiresLibrary && !hasLibrary ? "需要先在主窗口导入资料库" : tool.description} onClick={() => setActive(tool.id)}><span className="rt-nav-icon"><tool.icon size={15} strokeWidth={1.7} /></span><span><strong>{tool.label}</strong><small>{tool.description}</small></span></button>)}</section>)}</nav></aside><main className="rt-content">{!definition.fullBleed && <header className="rt-content-header"><h2>{definition.label}</h2><p>{definition.description}</p></header>}<div className="rt-stack">{!library.loaded ? <div className="rt-empty" role="status">正在读取应用状态…</div> : tools.filter(tool => visited.has(tool.id)).map(tool => <section key={tool.id} className={`rt-panel${tool.fullBleed ? " rt-full" : ""}`} hidden={tool.id !== active} aria-label={tool.label}><Suspense fallback={<div className="rt-empty" role="status">正在加载工具…</div>}><tool.component key={library.snapshot?.dataDirectory ?? "unconfigured"} active={tool.id === active} /></Suspense></section>)}</div></main></div></div><ConfirmationDialog /><Notices /></Tooltip.Provider>;
}
export default ToolboxWindow;
