import { listen } from "@tauri-apps/api/event";
import { confirm as confirmDialog, open, save } from "@tauri-apps/plugin-dialog";

import {
  exportWorkbook,
  getAppSnapshot,
  importImages,
  importWorkbook,
  initializeDataDirectory,
  migrateDataDirectory,
  openDataDirectory,
  type AppSnapshot,
  type ImageImportProgress,
} from "../api";

export type ViewMode = "gallery" | "table";

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
  /** 资料库行集合变化（导入/删除）时 +1，数据视图据此整体重载 */
  dataVersion: 0,
  /** 文件夹/压缩包导入进行中的进度，空闲时为 null */
  importProgress: null as ImageImportProgress | null,
  /** 库内查重视图是否打开 */
  dedupeOpen: false,
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
    app.snapshot =
      mode === "initialize"
        ? await initializeDataDirectory(selection)
        : await openDataDirectory(selection);
    setNotice({ tone: "success", text: "数据目录已连接。" });
  });
}

export async function chooseWorkbook(): Promise<void> {
  const selection = await open({
    multiple: false,
    directory: false,
    title: "导入 NovelAI Metadata 工作簿（追加进资料库）",
    filters: [{ name: "Excel 工作簿", extensions: ["xlsx"] }],
  });
  if (typeof selection !== "string") {
    return;
  }
  await runAction(async () => {
    const result = await importWorkbook(selection);
    app.snapshot = result.snapshot;
    if (result.added > 0) {
      app.dataVersion += 1;
    }
    const parts = [`新增 ${formatCount(result.added)} 行`];
    if (result.skippedExisting > 0) {
      parts.push(`跳过 ${formatCount(result.skippedExisting)} 行已存在`);
    }
    if (result.changedExisting > 0) {
      parts.push(`其中 ${formatCount(result.changedExisting)} 行源文件有变化（未改动库内数据）`);
    }
    if (result.embeddedImagesStored > 0) {
      parts.push(`提取 ${formatCount(result.embeddedImagesStored)} 张嵌入图片`);
    }
    setNotice({ tone: "success", text: `导入完成：${parts.join("，")}。` });
  });
}

export async function chooseImageFolder(): Promise<void> {
  const selection = await open({
    directory: true,
    multiple: false,
    title: "选择要导入的图片文件夹（追加进资料库）",
  });
  if (typeof selection !== "string") {
    return;
  }
  await runImageImport(selection);
}

export async function chooseImageArchive(): Promise<void> {
  const selection = await open({
    multiple: false,
    directory: false,
    title: "选择要导入的压缩包（追加进资料库）",
    filters: [{ name: "压缩包", extensions: ["zip", "7z", "rar"] }],
  });
  if (typeof selection !== "string") {
    return;
  }
  await runImageImport(selection);
}

async function runImageImport(path: string): Promise<void> {
  await runAction(async () => {
    const unlisten = await listen<ImageImportProgress>(
      "import-images://progress",
      event => {
        app.importProgress = event.payload;
      },
    );
    try {
      const result = await importImages(path);
      app.snapshot = result.snapshot;
      if (result.added > 0) {
        app.dataVersion += 1;
      }
      const parts = [`新增 ${formatCount(result.added)} 行`];
      if (result.skippedExisting > 0) {
        parts.push(`跳过 ${formatCount(result.skippedExisting)} 张已入库`);
      }
      if (result.changedExisting > 0) {
        parts.push(
          `其中 ${formatCount(result.changedExisting)} 张源文件有变化（未改动库内数据）`,
        );
      }
      if (result.metadataFailed > 0) {
        parts.push(`${formatCount(result.metadataFailed)} 张元数据解析失败（已入库并标记）`);
      }
      setNotice({
        tone: "success",
        text: `导入完成（共发现 ${formatCount(result.totalFound)} 张）：${parts.join("，")}。`,
      });
    } finally {
      unlisten();
      app.importProgress = null;
    }
  });
}

export async function chooseExport(): Promise<void> {
  const library = app.snapshot?.library;
  if (!library || library.rowCount === 0) {
    return;
  }
  const lastSource = library.lastBatch?.sourcePath ?? "";
  const baseName =
    lastSource
      .split(/[\\/]/)
      .pop()
      ?.replace(/\.xlsx$/i, "") || "smart-spreadsheet";
  const selection = await save({
    title: "导出新的 Excel 副本（不覆盖已有文件）",
    defaultPath: `${baseName}-tagged.xlsx`,
    filters: [{ name: "Excel 工作簿", extensions: ["xlsx"] }],
  });
  if (typeof selection !== "string") {
    return;
  }
  await runAction(async () => {
    const result = await exportWorkbook(selection);
    setNotice({
      tone: "success",
      text: `已导出 ${formatCount(result.rowCount)} 行到 ${result.path}`,
    });
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
