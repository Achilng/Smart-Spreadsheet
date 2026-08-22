import { emitTo, listen } from "@tauri-apps/api/event";
import { confirm as confirmDialog, open } from "@tauri-apps/plugin-dialog";

import {
  backfillPerceptualHashes,
  backfillVibeStatuses,
  getAppSnapshot,
  initializeDataDirectory,
  migrateDataDirectory,
  openDataDirectory,
  resetConfiguration,
  resetData as apiResetData,
  type AppSnapshot,
  type ContentHashProgress,
  type ExportProgress,
  type ImageImportProgress,
  type MigrationProgress,
  type PerceptualHashProgress,
  type VibeStatusProgress,
} from "../api";

export type ViewMode = "group" | "gallery" | "table" | "duplicates" | "promptDocs";

export interface Notice {
  tone: "error" | "success";
  text: string;
}

export interface QueuedNotice extends Notice {
  id: number;
}

export const app = $state({
  snapshot: null as AppSnapshot | null,
  loaded: false,
  busy: false,
  /** 通知队列：最多同时展示 3 条；error 不自动消失，success 5 秒后过期 */
  notices: [] as QueuedNotice[],
  viewMode: "gallery" as ViewMode,
  detailOpen: true,
  galleryCardSize: 190,
  tableRowHeight: 64,
  /** 资料库行集合变化（导入/删除）时 +1，数据视图据此整体重载 */
  dataVersion: 0,
  /** 本次行集合变化是否应保留视图滚动位置 */
  preserveScrollOnDataChange: false,
  /** 本次变化只改行内值（撤销/重做等），选区与缩略图缓存仍有效 */
  preserveSelectionOnDataChange: false,
  /** 文件夹/压缩包导入进行中的进度，空闲时为 null */
  importProgress: null as ImageImportProgress | null,
  /** 当前追加导入会在写库后自动运行库内画师前缀检查。 */
  autoArtistPrefixImportActive: false,
  /** “更新现有图片”说明与来源选择弹窗 */
  updateImportOpen: false,
  /** 打开旧目录时历史图片内容哈希补算进度，空闲时为 null */
  hashProgress: null as ContentHashProgress | null,
  /** 导出（xlsx / JSON / 图片文件）进行中的进度，空闲时为 null */
  exportProgress: null as ExportProgress | null,
  /** 感知哈希补算进度，空闲时为 null */
  phashProgress: null as PerceptualHashProgress | null,
  /** 升级后首启的 VIBE 索引回填进度，空闲时为 null */
  vibeBackfillProgress: null as VibeStatusProgress | null,
  /** 数据目录迁移的分阶段进度，空闲时为 null */
  migrationProgress: null as MigrationProgress | null,
  /** 分组管理视图是否打开 */
  groupManageOpen: false,
  /** Discord 风格资料库筛选面板是否打开。 */
  filterOpen: false,
});

let noticeSerial = 1;
const noticeTimers = new Map<number, number>();
let clearHistoryCallback: () => void = () => {};

/** 由历史模块在加载时注册，避免 app-state 反向依赖历史执行器。 */
export function registerHistoryClearer(clearer: () => void): void {
  clearHistoryCallback = clearer;
}

/**
 * 入队一条通知（null 表示清空全部）。
 * 通知按队列堆叠而不是互相覆盖；success 5 秒后自动过期，error 常驻到手动关闭。
 */
export function setNotice(notice: Notice | null): void {
  if (notice === null) {
    for (const timer of noticeTimers.values()) {
      window.clearTimeout(timer);
    }
    noticeTimers.clear();
    app.notices = [];
    return;
  }
  const queued: QueuedNotice = { ...notice, id: noticeSerial };
  noticeSerial += 1;
  // 最多同时保留 3 条，挤掉最旧的一条
  const next = [...app.notices, queued];
  while (next.length > 3) {
    const removed = next.shift();
    if (removed) {
      const timer = noticeTimers.get(removed.id);
      if (timer !== undefined) {
        window.clearTimeout(timer);
        noticeTimers.delete(removed.id);
      }
    }
  }
  app.notices = next;
  if (queued.tone === "success") {
    const timer = window.setTimeout(() => {
      dismissNotice(queued.id);
    }, 5000);
    noticeTimers.set(queued.id, timer);
  }
}

export function dismissNotice(id: number): void {
  const timer = noticeTimers.get(id);
  if (timer !== undefined) {
    window.clearTimeout(timer);
    noticeTimers.delete(id);
  }
  app.notices = app.notices.filter(notice => notice.id !== id);
}

/**
 * 通知数据视图重载行集合。删除只是就地移除行，可保留滚动位置；
 * 导入、换库等数据集整体变化继续回顶。
 * preserveSelection：仅值变化（撤销/重做、批量编辑）时为 true——行集合没变，
 * 选区与缩略图缓存仍然有效，不应被清空（否则每次 Ctrl+Z 都会丢选区、闪图）。
 */
export function bumpDataVersion(
  options: { preserveScroll?: boolean; preserveSelection?: boolean; origin?: "main" | "toolbox" } = {},
): void {
  app.preserveScrollOnDataChange = options.preserveScroll ?? false;
  app.preserveSelectionOnDataChange = options.preserveSelection ?? false;
  app.dataVersion += 1;
  // 同步告知工具箱窗口资料库已变化（未打开时静默失败）。
  // origin 让工具箱区分“主窗口自己的编辑”（需失效工具箱撤销栈）与
  // “工具箱操作回流的通知”（不能反过来清掉工具箱刚记下的撤销）。
  void emitTo("toolbox", "main://library-changed", options.origin ?? "main").catch(() => {});
}

export async function refreshSnapshot(): Promise<void> {
  app.busy = true;
  try {
    app.snapshot = await getAppSnapshot();
  } catch (error) {
    setNotice({ tone: "error", text: errorText(error) });
  } finally {
    app.busy = false;
    app.loaded = true;
  }
}

export async function resetAndReconfigure(): Promise<void> {
  await runAction(async () => {
    app.snapshot = await resetConfiguration();
    clearOperationHistory();
  });
}

export async function resetDataWithConfirmation(): Promise<void> {
  const confirmed = await confirmDialog(
    "将清空所有已导入的数据（图片副本、缩略图、数据库），回到初始导入页面。原始图片文件不受影响。此操作不可撤销，是否继续？",
    { title: "重置表格", kind: "warning", okLabel: "确认重置", cancelLabel: "取消" },
  );
  if (!confirmed) return;
  await runAction(async () => {
    app.snapshot = await apiResetData();
    clearOperationHistory();
    bumpDataVersion();
    await notifyMainStateChanged("reset");
    setNotice({ tone: "success", text: "表格已重置，请重新导入数据。" });
  });
}

export async function chooseDirectory(mode: "initialize" | "open"): Promise<void> {
  const selection = await open({
    directory: true,
    multiple: false,
    title: mode === "initialize" ? "选择空的数据目录" : "打开智能表格数据目录",
  });
  if (typeof selection !== "string") {
    return;
  }
  await runAction(async () => {
    if (mode === "initialize") {
      app.snapshot = await initializeDataDirectory(selection);
    } else {
      const unlisten = await listen<ContentHashProgress>("content-hash://progress", event => {
        app.hashProgress = event.payload;
      });
      try {
        app.snapshot = await openDataDirectory(selection);
      } finally {
        unlisten();
        app.hashProgress = null;
      }
      // 换到的旧库可能还没有 VIBE 聚合索引，后台补齐（已就绪时立即返回）。
      void runVibeBackfill();
    }
    clearOperationHistory();
    setNotice({ tone: "success", text: "数据目录已连接。" });
  });
}

let vibeBackfillActive = false;

/**
 * 在后台补齐历史图片的 VIBE 数量与组合签名（升级后首启一次性工作）。
 * 无待补行时后端只做一次查询立即返回，因此启动和换库后都可以放心调用；
 * 不占用 app.busy，进度显示在右下角，完成且确实有补齐时刷新数据视图
 * 并给出一次性说明。失败不阻塞使用，下次启动自动重试剩余行。
 */
export async function runVibeBackfill(): Promise<void> {
  if (vibeBackfillActive) return;
  if (!app.snapshot?.dataDirectory || app.snapshot.startupError) return;
  vibeBackfillActive = true;
  try {
    const unlisten = await listen<VibeStatusProgress>("vibe-status://progress", event => {
      app.vibeBackfillProgress = event.payload;
    });
    try {
      const result = await backfillVibeStatuses();
      if (result.total > 0) {
        bumpDataVersion({ preserveScroll: true, preserveSelection: true });
        setNotice({
          tone: "success",
          text: `已为 ${formatCount(result.total)} 张历史图片补齐 VIBE 聚合索引与作画模型信息${result.unreadable > 0 ? `（${formatCount(result.unreadable)} 张原图不可读，已跳过）` : ""}，重复视图现在可以按 VIBE 分组，预览图左上角会显示模型版本徽章。`,
        });
      }
    } finally {
      unlisten();
      app.vibeBackfillProgress = null;
    }
  } catch (error) {
    setNotice({ tone: "error", text: `VIBE 聚合索引建立失败：${errorText(error)}` });
  } finally {
    vibeBackfillActive = false;
  }
}

export async function runPhashBackfill(): Promise<void> {
  await runAction(async () => {
    const unlisten = await listen<PerceptualHashProgress>(
      "perceptual-hash://progress",
      event => {
        app.phashProgress = event.payload;
      },
    );
    try {
      const result = await backfillPerceptualHashes();
      if (result.total === 0) {
        setNotice({ tone: "success", text: "所有图片的感知哈希已是最新。" });
      } else {
        setNotice({
          tone: "success",
          text: `感知哈希更新完成：共 ${formatCount(result.total)} 张，成功 ${formatCount(result.updated)} 张${result.unreadable > 0 ? `，${formatCount(result.unreadable)} 张不可读` : ""}。`,
        });
      }
    } finally {
      unlisten();
      app.phashProgress = null;
    }
  });
}

export async function chooseMigration(): Promise<void> {
  if (!app.snapshot?.dataDirectory) {
    return;
  }
  const confirmed = await confirmDialog(
    "迁移会复制并校验数据库、工作簿和缓存，切换成功后再清理旧目录。目标必须是空文件夹；失败时应用继续使用当前目录。是否继续？",
    { title: "迁移数据目录", kind: "warning", okLabel: "继续", cancelLabel: "取消" },
  );
  if (!confirmed) {
    return;
  }
  const selection = await open({
    directory: true,
    multiple: false,
    title: "选择空的数据迁移目标目录",
  });
  if (typeof selection !== "string") {
    return;
  }
  await runAction(async () => {
    const unlisten = await listen<MigrationProgress>("migration://progress", event => {
      app.migrationProgress = event.payload;
    });
    app.migrationProgress = {
      stage: "preparing",
      completed: 0,
      total: 0,
      stageCompleted: 0,
      stageTotal: 0,
    };
    try {
      const result = await migrateDataDirectory(selection);
      app.snapshot = result.snapshot;
      clearOperationHistory();
      await notifyMainStateChanged("migrated");
      setNotice(
        result.retiredSource
          ? { tone: "error", text: `迁移成功，但旧目录未能自动清理：${result.retiredSource}` }
          : { tone: "success", text: `数据目录已迁移到 ${selection}` },
      );
    } finally {
      unlisten();
      app.migrationProgress = null;
    }
  });
}

export type MainStateChange = "migrated" | "reset" | "libraryEdited";

export async function notifyMainStateChanged(kind: MainStateChange): Promise<void> {
  try {
    await emitTo("main", "toolbox://app-state-changed", kind);
  } catch {
    // 主窗口可能已经关闭；数据操作本身已经成功，不应被通知失败反向判为失败。
  }
}

export async function runAction(action: () => Promise<void>): Promise<void> {
  app.busy = true;
  setNotice(null);
  try {
    await action();
  } catch (error) {
    setNotice({ tone: "error", text: errorText(error) });
  } finally {
    app.busy = false;
  }
}

function clearOperationHistory(): void {
  clearHistoryCallback();
}

export function formatCount(value: number): string {
  return value.toLocaleString("zh-CN");
}

export function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
