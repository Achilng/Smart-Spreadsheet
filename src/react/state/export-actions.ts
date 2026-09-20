import { create } from "zustand";
import { open, save } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { exportXlsx, exportZhihuijiJson, exportPromptRotationJson, exportImageFiles, type ExportProgress, type JsonExportOptions, type RowSelection } from "../../lib/api";
import { snapshotQueryFilters } from "../../lib/utils/library-query";
import { errorText, formatCount } from "../../lib/utils/format";
import { useRows } from "./library";
import { selectedCount, selectionDto, useSelection } from "./selection";
import { runTask, useTasks } from "./tasks";
import { notify } from "./notices";
import type { MenuItem } from "../ui/controls";

export const useJsonExport = create<{ request: { selection: RowSelection; label: string } | null }>(() => ({ request: null }));
export function exportScope(): RowSelection {
  return selectedCount(useSelection.getState()) > 0 ? selectionDto() : { kind: "filtered", ...snapshotQueryFilters(useRows.getState().query), excludedRowIds: [] };
}
export function exportScopeLabel(): string {
  const count = selectedCount(useSelection.getState());
  return count > 0 ? `已选 ${formatCount(count)} 张图片` : "当前筛选结果";
}
type Format = "xlsx" | "rotation" | "copy" | "hardlink";

async function withProgress(action: () => Promise<void>): Promise<void> {
  const stop = await listen<ExportProgress>("export://progress", event => useTasks.setState({ progress: event.payload }));
  try { await action(); } finally { stop(); useTasks.setState({ progress: null }); }
}

async function chooseExport(format: Format): Promise<void> {
  if (format === "rotation" && !selectedCount(useSelection.getState())) { notify("请先选择要导出的图片", "error"); return; }
  const selection = exportScope();
  try {
    await runTask("导出图片资料", async () => {
      const path = format === "copy" || format === "hardlink" ? await open({ directory: true, multiple: false, title: "选择导出位置（将在其中新建输出文件夹）" })
        : await save({ title: format === "xlsx" ? "导出新的 xlsx（不覆盖已有文件）" : "导出轮询脚本 JSON（已有文件会被替换）", defaultPath: format === "xlsx" ? "智能表格导出.xlsx" : "NovelAI轮询项目.json", filters: [{ name: format === "xlsx" ? "Excel 工作簿" : "JSON 文件", extensions: [format === "xlsx" ? "xlsx" : "json"] }] });
      if (typeof path !== "string") return;
      await withProgress(async () => {
        if (format === "xlsx") {
          const result = await exportXlsx(selection, path);
          notify(`已导出 ${formatCount(result.rowCount)} 行到 ${result.path}${result.imageFailures ? `，${result.imageFailures} 行无可用图片（仅导出文字）` : ""}`);
        } else if (format === "rotation") {
          const result = await exportPromptRotationJson(selection, path); notify(`已将 ${formatCount(result.exported)} 张图片导出为轮询项目：${result.path}`);
        } else {
          const result = await exportImageFiles(selection, path, format);
          notify(`已导出 ${formatCount(result.exported)} 张图片到 ${result.directory}${result.hardlinkFallbacks ? `，${result.hardlinkFallbacks} 张硬链接失败已改为复制` : ""}${result.missing ? `，${result.missing} 行找不到图片` : ""}`, result.missing ? "error" : "success");
        }
      });
    });
  } catch (error) { notify(`导出失败：${errorText(error)}`, "error"); }
}

/** False means the native save dialog was cancelled; the options stay open. */
export async function executeJsonExport(selection: RowSelection, options: JsonExportOptions): Promise<boolean> {
  return runTask("导出智绘姬 JSON", async () => {
    const path = await save({ title: "导出智绘姬 JSON（已有文件会被替换）", defaultPath: "智绘姬预设.json", filters: [{ name: "JSON 文件", extensions: ["json"] }] });
    if (!path) return false;
    await withProgress(async () => {
      const result = await exportZhihuijiJson(selection, path, options);
      notify(`已导出 ${formatCount(result.exported)} 条预设到 ${result.path}${result.duplicatesRemoved ? `，去除 ${result.duplicatesRemoved} 条重复` : ""}${result.artistsAdded ? `，为 ${result.artistsAdded} 条补齐画师` : ""}`);
    });
    return true;
  });
}

export function buildExportItems(): MenuItem[] {
  const hint = `导出${exportScopeLabel()}`;
  return [
    { label: "导出 xlsx", hint, action: () => void chooseExport("xlsx") },
    { label: "导出智绘姬 JSON", hint, action: () => useJsonExport.setState({ request: { selection: exportScope(), label: exportScopeLabel() } }) },
    { label: "导出为轮询脚本 JSON", hint: selectedCount(useSelection.getState()) ? `${hint} · 导入轮询脚本后逐张复现` : "请先选择要导出的图片", action: () => void chooseExport("rotation") },
    { label: "导出图片（复制）", hint, action: () => void chooseExport("copy") },
    { label: "导出图片（硬链接）", hint: `${hint} · 同盘秒出，失败自动复制`, action: () => void chooseExport("hardlink") },
  ];
}
