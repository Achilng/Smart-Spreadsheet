import { confirm as confirmDialog, open, save } from "@tauri-apps/plugin-dialog";

import {
  exportWorkbook,
  getAppSnapshot,
  importWorkbook,
  initializeDataDirectory,
  migrateDataDirectory,
  openDataDirectory,
  type AppSnapshot,
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
  /** 工作簿被导入/替换时 +1，数据视图据此整体重载 */
  dataVersion: 0,
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
  if (app.snapshot?.workbook) {
    const confirmed = await confirmDialog(
      "替换当前工作簿会清除现有行与 Tag 数据。原 Excel 不会被修改。是否继续？",
      { title: "替换工作簿", kind: "warning", okLabel: "继续", cancelLabel: "取消" },
    );
    if (!confirmed) {
      return;
    }
  }
  const selection = await open({
    multiple: false,
    directory: false,
    title: "选择 NovelAI Metadata 工作簿",
    filters: [{ name: "Excel 工作簿", extensions: ["xlsx"] }],
  });
  if (typeof selection !== "string") {
    return;
  }
  await runAction(async () => {
    const result = await importWorkbook(selection);
    app.snapshot = result.snapshot;
    app.dataVersion += 1;
    setNotice({
      tone: "success",
      text: `已导入 ${formatCount(result.importedRows)} 行，识别 ${formatCount(result.embeddedImages)} 张嵌入图片。`,
    });
  });
}

export async function chooseExport(): Promise<void> {
  const workbook = app.snapshot?.workbook;
  if (!workbook) {
    return;
  }
  const baseName = workbook.importedName.replace(/\.xlsx$/i, "") || "smart-spreadsheet";
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
