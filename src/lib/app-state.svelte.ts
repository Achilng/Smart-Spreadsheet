import { listen } from "@tauri-apps/api/event";
import { confirm as confirmDialog, open } from "@tauri-apps/plugin-dialog";

import {
  backfillPerceptualHashes,
  getAppSnapshot,
  initializeDataDirectory,
  migrateDataDirectory,
  openDataDirectory,
  resetConfiguration,
  resetData as apiResetData,
  searchSimilarImages,
  type AppSnapshot,
  type ContentHashProgress,
  type ExportProgress,
  type ImageImportProgress,
  type PerceptualHashProgress,
  type SimilarImageMatch,
} from "../api";

export type ViewMode = "group" | "gallery" | "table" | "duplicates";

export interface Notice {
  tone: "error" | "success";
  text: string;
}

export const app = $state({
  snapshot: null as AppSnapshot | null,
  loaded: false,
  busy: false,
  notice: null as Notice | null,
  viewMode: "gallery" as ViewMode,
  detailOpen: true,
  galleryCardSize: 190,
  tableRowHeight: 64,
  /** 资料库行集合变化（导入/删除）时 +1，数据视图据此整体重载 */
  dataVersion: 0,
  /** 文件夹/压缩包导入进行中的进度，空闲时为 null */
  importProgress: null as ImageImportProgress | null,
  /** 打开旧目录时历史图片内容哈希补算进度，空闲时为 null */
  hashProgress: null as ContentHashProgress | null,
  /** 导出（xlsx / JSON / 图片文件）进行中的进度，空闲时为 null */
  exportProgress: null as ExportProgress | null,
  /** 智绘姬 JSON 去重工具是否打开 */
  jsonDedupeOpen: false,
  /** 随机画师串生成器是否打开 */
  artistGenOpen: false,
  /** 感知哈希补算进度，空闲时为 null */
  phashProgress: null as PerceptualHashProgress | null,
  /** 以图搜图结果，空闲时为 null */
  searchResults: null as SimilarImageMatch[] | null,
  /** 以图搜图的查询图片路径 */
  searchQueryPath: null as string | null,
  /** 建议分组视图是否打开 */
  groupSuggestOpen: false,
  /** 分组管理视图是否打开 */
  groupManageOpen: false,
});

let noticeTimer = 0;

export function setNotice(notice: Notice | null): void {
  app.notice = notice;
  window.clearTimeout(noticeTimer);
  if (notice?.tone === "success") {
    noticeTimer = window.setTimeout(() => {
      app.notice = null;
    }, 5000);
  }
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
    app.dataVersion += 1;
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
    }
    setNotice({ tone: "success", text: "数据目录已连接。" });
  });
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

export async function chooseSearchImage(): Promise<void> {
  const selection = await open({
    multiple: false,
    directory: false,
    title: "选择用于搜索的图片",
    filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "bmp", "gif", "webp", "tiff"] }],
  });
  if (typeof selection !== "string") {
    return;
  }
  await runAction(async () => {
    const matches = await searchSimilarImages(selection, 10);
    app.searchQueryPath = selection;
    app.searchResults = matches;
    if (matches.length === 0) {
      setNotice({ tone: "success", text: "未找到相似图片。请先刷新感知哈希后再试。" });
    } else {
      setNotice({
        tone: "success",
        text: `找到 ${formatCount(matches.length)} 张相似图片。`,
      });
    }
  });
}

export function closeSearchResults(): void {
  app.searchResults = null;
  app.searchQueryPath = null;
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
    const result = await migrateDataDirectory(selection);
    app.snapshot = result.snapshot;
    setNotice(
      result.retiredSource
        ? { tone: "error", text: `迁移成功，但旧目录未能自动清理：${result.retiredSource}` }
        : { tone: "success", text: `数据目录已迁移到 ${selection}` },
    );
  });
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

export function formatCount(value: number): string {
  return value.toLocaleString("zh-CN");
}

export function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
