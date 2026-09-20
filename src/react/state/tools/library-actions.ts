import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { confirm, open } from "@tauri-apps/plugin-dialog";
import { backfillPerceptualHashes, getAppSnapshot, migrateDataDirectory, resetData, type MigrationProgress, type PerceptualHashProgress } from "../../../lib/api";
import { clearOperationHistory } from "../../../lib/stores/history-context";
import { notifyCompareLibraryReset, notifyMainStateChanged } from "../../../lib/windows/library-events";
import { errorText, formatCount } from "../../../lib/utils/format";
import { useLibrary } from "../library";
import { notify } from "../notices";
import { runTask } from "../tasks";

export const useToolProgress = create<{ migration: MigrationProgress | null; phash: PerceptualHashProgress | null }>(() => ({ migration: null, phash: null }));
export async function toolAction(label: string, action: () => Promise<void>) {
  try { await runTask(label, action); } catch (error) { notify(errorText(error), "error"); }
}
let refreshPending: Promise<void> | null = null;
export function refreshToolboxSnapshot(): Promise<void> {
  if (refreshPending) return refreshPending;
  refreshPending = getAppSnapshot().then(snapshot => { useLibrary.setState({ snapshot, loaded: true, error: null }); })
    .catch(error => { useLibrary.setState({ loaded: true, error: errorText(error) }); notify(errorText(error), "error"); })
    .finally(() => { refreshPending = null; });
  return refreshPending;
}
export async function chooseMigration() {
  if (!useLibrary.getState().snapshot?.dataDirectory) return;
  try {
    if (!await confirm("迁移会复制并校验数据库、工作簿和缓存，切换成功后再清理旧目录。目标必须是空文件夹；失败时应用继续使用当前目录。是否继续？", { title: "迁移数据目录", kind: "warning", okLabel: "继续", cancelLabel: "取消" })) return;
    const destination = await open({ directory: true, multiple: false, title: "选择空的数据迁移目标目录" });
    if (typeof destination !== "string") return;
    await toolAction("迁移资料库", async () => {
      let unlisten: (() => void) | undefined;
      try {
        unlisten = await listen<MigrationProgress>("migration://progress", event => useToolProgress.setState({ migration: event.payload }));
        useToolProgress.setState({ migration: { stage: "preparing", completed: 0, total: 0, stageCompleted: 0, stageTotal: 0 } });
        const result = await migrateDataDirectory(destination);
        useLibrary.setState({ snapshot: result.snapshot }); clearOperationHistory(); notifyCompareLibraryReset(); await notifyMainStateChanged("migrated");
        notify(result.retiredSource ? `迁移成功，但旧目录未能自动清理：${result.retiredSource}` : `数据目录已迁移到 ${destination}`, result.retiredSource ? "error" : "success");
      } finally { unlisten?.(); useToolProgress.setState({ migration: null }); }
    });
  } catch (error) { notify(errorText(error), "error"); }
}
export async function resetDataWithConfirmation() {
  try {
    if (!await confirm("将清空所有已导入的数据（图片副本、缩略图、数据库），回到初始导入页面。原始图片文件不受影响。此操作不可撤销，是否继续？", { title: "重置表格", kind: "warning", okLabel: "确认重置", cancelLabel: "取消" })) return;
    await toolAction("重置表格", async () => {
      useLibrary.setState({ snapshot: await resetData() }); clearOperationHistory(); notifyCompareLibraryReset(); await notifyMainStateChanged("reset"); notify("表格已重置，请重新导入数据。");
    });
  } catch (error) { notify(errorText(error), "error"); }
}
export async function runPhashBackfill() {
  await toolAction("刷新感知哈希", async () => {
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await listen<PerceptualHashProgress>("perceptual-hash://progress", event => useToolProgress.setState({ phash: event.payload }));
      const result = await backfillPerceptualHashes();
      notify(result.total === 0 ? "所有图片的感知哈希已是最新。" : `感知哈希更新完成：共 ${formatCount(result.total)} 张，成功 ${formatCount(result.updated)} 张${result.unreadable ? `，${formatCount(result.unreadable)} 张不可读` : ""}。`);
    } finally { unlisten?.(); useToolProgress.setState({ phash: null }); }
  });
}
