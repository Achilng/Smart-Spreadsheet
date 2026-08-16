import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";

import {
  exportImageFiles,
  exportXlsx,
  exportZhihuijiJson,
  type ExportProgress,
  type ImageFileExportMode,
  type JsonExportOptions,
  type RowSelection,
} from "../api";
import { app, formatCount, runAction, setNotice } from "./app-state.svelte";
import { requestJsonExport } from "./json-export-dialog.svelte";
import { rowStore } from "./row-store.svelte";
import { getSelectedCount, selectionDto } from "./selection-store.svelte";

/**
 * 导出范围：有勾选时导出勾选行，否则导出当前筛选结果
 * （无筛选 Tag 时即整个资料库）。
 */
export function exportScope(): RowSelection {
  if (getSelectedCount() > 0) {
    return selectionDto();
  }
  return {
    kind: "filtered",
    tags: [...rowStore.tags],
    tagMode: rowStore.tagMode,
    dedupe: rowStore.dedupe,
    singleArtistOnly: rowStore.singleArtistOnly,
    artistFilter: rowStore.artistFilter,
    hasVibe: rowStore.hasVibe,
    untaggedOnly: rowStore.untaggedOnly,
    search: rowStore.search,
    excludedRowIds: [],
  };
}

/** 导出范围的界面描述，用于菜单提示。 */
export function exportScopeLabel(): string {
  const selected = getSelectedCount();
  if (selected > 0) {
    return `已选 ${formatCount(selected)} 行`;
  }
  const filtered =
    rowStore.tags.length > 0 ||
    rowStore.dedupe !== "none" ||
    rowStore.singleArtistOnly ||
    rowStore.artistFilter !== "" ||
    rowStore.hasVibe ||
    rowStore.untaggedOnly ||
    rowStore.search.trim().length > 0;
  return filtered ? "当前筛选结果" : "全部行";
}

/** 四种导出方式的菜单项工厂（TopBar 导出 / 底部选择条"导出所选"共用）。 */
export function buildExportItems(): {
  label: string;
  hint?: string;
  action: () => void;
}[] {
  const scopeHint = `导出${exportScopeLabel()}`;
  return [
    { label: "导出 xlsx", hint: scopeHint, action: () => void chooseXlsxExport() },
    { label: "导出智绘姬 JSON", hint: scopeHint, action: () => void chooseJsonExport() },
    {
      label: "导出图片（复制）",
      hint: scopeHint,
      action: () => void chooseImageFilesExport("copy"),
    },
    {
      label: "导出图片（硬链接）",
      hint: `${scopeHint} · 同盘秒出，失败自动复制`,
      action: () => void chooseImageFilesExport("hardlink"),
    },
  ];
}

export async function chooseXlsxExport(): Promise<void> {
  if (!hasRows()) {
    return;
  }
  const destination = await save({
    title: "导出新的 xlsx（不覆盖已有文件）",
    defaultPath: "智能表格导出.xlsx",
    filters: [{ name: "Excel 工作簿", extensions: ["xlsx"] }],
  });
  if (typeof destination !== "string") {
    return;
  }
  const scope = exportScope();
  await runExport(async () => {
    const result = await exportXlsx(scope, destination);
    const parts = [`已导出 ${formatCount(result.rowCount)} 行到 ${result.path}`];
    if (result.imageFailures > 0) {
      parts.push(`${formatCount(result.imageFailures)} 行无可用图片（仅导出文字）`);
    }
    setNotice({ tone: "success", text: parts.join("，") });
  });
}

export async function chooseJsonExport(): Promise<void> {
  if (!hasRows()) {
    return;
  }
  requestJsonExport(exportScope(), exportScopeLabel());
}

export async function executeJsonExport(
  scope: RowSelection,
  options: JsonExportOptions,
): Promise<void> {
  const destination = await save({
    title: "导出智绘姬 JSON（已有文件会被替换）",
    defaultPath: "智绘姬预设.json",
    filters: [{ name: "JSON 文件", extensions: ["json"] }],
  });
  if (typeof destination !== "string") return;

  await runExport(async () => {
    const result = await exportZhihuijiJson(scope, destination, options);
    const parts = [`已导出 ${formatCount(result.exported)} 条预设到 ${result.path}`];
    if (result.duplicatesRemoved > 0) {
      parts.push(`去除 ${formatCount(result.duplicatesRemoved)} 条重复`);
    }
    if (result.artistsAdded > 0) {
      parts.push(`为 ${formatCount(result.artistsAdded)} 条补齐画师`);
    }
    setNotice({
      tone: "success",
      text: parts.join("，"),
    });
  });
}

export async function chooseImageFilesExport(mode: ImageFileExportMode): Promise<void> {
  if (!hasRows()) {
    return;
  }
  const parentDir = await open({
    directory: true,
    multiple: false,
    title: "选择导出图片的存放位置（将在其中新建输出文件夹）",
  });
  if (typeof parentDir !== "string") {
    return;
  }
  const scope = exportScope();
  await runExport(async () => {
    const result = await exportImageFiles(scope, parentDir, mode);
    const parts = [`已导出 ${formatCount(result.exported)} 张图片到 ${result.directory}`];
    if (result.hardlinkFallbacks > 0) {
      parts.push(`${formatCount(result.hardlinkFallbacks)} 张硬链接失败已改为复制`);
    }
    if (result.missing > 0) {
      parts.push(`${formatCount(result.missing)} 行找不到图片文件`);
    }
    setNotice({ tone: "success", text: parts.join("，") });
  });
}

function hasRows(): boolean {
  const library = app.snapshot?.library;
  return Boolean(library && library.rowCount > 0);
}

async function runExport(action: () => Promise<void>): Promise<void> {
  await runAction(async () => {
    const unlisten = await listen<ExportProgress>("export://progress", event => {
      app.exportProgress = event.payload;
    });
    try {
      await action();
    } finally {
      unlisten();
      app.exportProgress = null;
    }
  });
}
