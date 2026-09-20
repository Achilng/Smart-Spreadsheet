import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { backfillStyleSignatures, backfillVibeStatuses, initializeDataDirectory, openDataDirectory, resetConfiguration, type AppSnapshot, type ContentHashProgress, type VibeStatusProgress } from "../../lib/api";
import { notifyCompareLibraryReset, notifyToolboxLibraryChanged } from "../../lib/windows/library-events";
import { errorText } from "../../lib/utils/format";
import { clearFieldDrafts } from "../ui/FieldEditor";
import { clearHistory } from "./history";
import { invalidateImageCache } from "./image-cache";
import { refreshTags, reloadRows, resetLibraryRows, useLibrary } from "./library";
import { clearSelection } from "./selection";
import { runAction, useTasks } from "./tasks";
import { notify } from "./notices";

export async function connectLibrary(snapshot: AppSnapshot): Promise<void> {
  resetLibraryRows(); clearSelection(); clearHistory(); clearFieldDrafts(); invalidateImageCache();
  useLibrary.setState({ snapshot, tags: [], tagError: null, error: null, loaded: true });
  notifyCompareLibraryReset(); notifyToolboxLibraryChanged("main");
  if (snapshot.dataDirectory && !snapshot.startupError) await Promise.all([reloadRows({ resetScroll: true }), refreshTags()]);
}
export async function chooseDirectory(mode: "initialize" | "open"): Promise<void> {
  try {
    const path = await open({ directory: true, multiple: false, title: mode === "initialize" ? "选择空的数据目录" : "打开智能表格数据目录" });
    if (typeof path !== "string") return;
    await runAction(async () => {
      let stop: (() => void) | undefined;
      try {
        if (mode === "open") stop = await listen<ContentHashProgress>("content-hash://progress", event => useTasks.setState({ progress: { processed: event.payload.processed, total: event.payload.total } }));
        await connectLibrary(await (mode === "initialize" ? initializeDataDirectory(path) : openDataDirectory(path)));
        notify("数据目录已连接。");
        void runStartupMaintenance();
      } finally { stop?.(); }
    }, "连接数据目录");
  } catch (error) { notify(`连接失败：${errorText(error)}`, "error"); }
}
export async function resetAndReconfigure(): Promise<void> {
  await runAction(async () => connectLibrary(await resetConfiguration()), "重新配置数据目录");
}

export const useMaintenance = create<{ label: string; progress: { processed: number; total: number; stage?: string } | null }>(() => ({ label: "", progress: null }));
let maintenance: Promise<void> | null = null;
export function runStartupMaintenance(): Promise<void> {
  if (maintenance) return maintenance;
  const directory = useLibrary.getState().snapshot?.dataDirectory;
  if (!directory || useLibrary.getState().snapshot?.startupError) return Promise.resolve();
  maintenance = (async () => {
    for (const [label, eventName, run] of [
      ["补齐 VIBE 索引", "vibe-status://progress", backfillVibeStatuses],
      ["补齐画风签名", "style-signature://progress", backfillStyleSignatures],
    ] as const) {
      if (useLibrary.getState().snapshot?.dataDirectory !== directory) break;
      let stop: (() => void) | undefined;
      try {
        useMaintenance.setState({ label, progress: null });
        stop = await listen<VibeStatusProgress>(eventName, event => useMaintenance.setState({ progress: event.payload }));
        const result = await run();
        if (result.total > 0 && useLibrary.getState().snapshot?.dataDirectory === directory) {
          await reloadRows({ keepActive: true }); notify(`已为 ${result.total.toLocaleString()} 张历史图片${label}。`);
        }
      } catch (error) { notify(`${label}失败：${errorText(error)}`, "error"); }
      finally { stop?.(); useMaintenance.setState({ label: "", progress: null }); }
    }
  })().finally(() => { maintenance = null; });
  return maintenance;
}
