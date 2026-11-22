import { useState } from "react";
import { Check, Download, FolderOutput } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { noteAgentExport } from "../../../lib/api/agent";
import {
  exportImageFiles, exportPromptRotationJson, exportSelectedImages, exportXlsx, exportZhihuijiJson,
  type ExportProgress, type ImageFileRenameMode,
} from "../../../lib/api/import-export";
import type { RowSelection } from "../../../lib/api/types";
import { errorText, formatCount } from "../../../lib/utils/format";
import { Button } from "../../ui/controls";
import { notify } from "../../state/notices";
import { ThumbStrip, useImageViewer } from "./AgentMedia";

export interface ExportDisplay {
  exportId: string;
  title: string;
  format: "xlsx" | "zhihuijiJson" | "rotationJson" | "copyImages" | "hardlinkImages" | "cleanImages" | string;
  options: Record<string, unknown>;
  rowIds: number[];
}

const FORMAT_LABELS: Record<string, string> = {
  xlsx: "带缩略图的 Excel 表格（.xlsx）",
  zhihuijiJson: "智绘姬预设 JSON",
  rotationJson: "轮询脚本 JSON",
  copyImages: "图片文件（复制到新文件夹）",
  hardlinkImages: "图片文件（硬链接到新文件夹）",
  cleanImages: "图片副本（可改名、可去掉生成信息）",
};
const FOLDER_FORMATS = new Set(["copyImages", "hardlinkImages", "cleanImages"]);

/** Windows-safe default file name from the card title. */
const fileName = (title: string, extension: string) => `${title.replace(/[\\/:*?"<>|]+/g, " ").trim().slice(0, 60) || "智能表格导出"}.${extension}`;

function optionLines(display: ExportDisplay): string[] {
  const options = display.options ?? {};
  const yes = (value: unknown) => (value ? "是" : "否");
  switch (display.format) {
    case "zhihuijiJson": return [
      `预设名用备注里的编号：${yes(options.noteNumberNames)}`,
      `画师串当作正向：${yes(options.artistsAsPositive)}`,
      `去掉最终正向相同的重复项：${yes(options.deduplicate)}`,
    ];
    case "cleanImages": {
      const rename = String(options.rename ?? "original");
      return [
        `文件名：${rename === "random" ? "随机名字" : rename === "custom" ? `按「${String(options.customName ?? "")}」加序号` : "保持原名"}`,
        `去掉图片里的生成信息：${yes(options.stripMetadata)}`,
      ];
    }
    case "hardlinkImages": return ["同一个磁盘上几乎瞬间完成、不占额外空间；做不到时自动改为复制"];
    case "copyImages": return ["会在你选的位置里新建一个文件夹放这些图"];
    case "rotationJson": return ["导入轮询脚本后可以逐张复现这些图"];
    default: return [];
  }
}

async function runExport(display: ExportDisplay, setProgress: (progress: ExportProgress | null) => void): Promise<string | null> {
  const selection: RowSelection = { kind: "explicit", rowIds: display.rowIds };
  const options = display.options ?? {};
  const folder = FOLDER_FORMATS.has(display.format);
  const json = display.format === "zhihuijiJson" || display.format === "rotationJson";
  const path = folder
    ? await open({ directory: true, multiple: false, title: "选择导出位置（会在里面新建输出文件夹）" })
    : await save({
      title: json ? "导出 JSON（已有文件会被替换）" : "导出新的 xlsx（不覆盖已有文件）",
      defaultPath: fileName(display.title, json ? "json" : "xlsx"),
      filters: [{ name: json ? "JSON 文件" : "Excel 工作簿", extensions: [json ? "json" : "xlsx"] }],
    });
  if (typeof path !== "string") return null;
  const stop = await listen<ExportProgress>("export://progress", event => setProgress(event.payload));
  try {
    switch (display.format) {
      case "xlsx": {
        const result = await exportXlsx(selection, path);
        return `已导出 ${formatCount(result.rowCount)} 行到 ${result.path}${result.imageFailures ? `，${result.imageFailures} 行没有可用图片（只导出了文字）` : ""}`;
      }
      case "zhihuijiJson": {
        const result = await exportZhihuijiJson(selection, path, {
          noteNumberNames: Boolean(options.noteNumberNames), artistsAsPositive: options.artistsAsPositive !== false, deduplicate: Boolean(options.deduplicate),
        });
        return `已导出 ${formatCount(result.exported)} 条预设到 ${result.path}${result.duplicatesRemoved ? `，去掉 ${result.duplicatesRemoved} 条重复` : ""}${result.emptyArtistsSkipped ? `，跳过 ${result.emptyArtistsSkipped} 条画师串为空的记录` : ""}`;
      }
      case "rotationJson": {
        const result = await exportPromptRotationJson(selection, path);
        return `已把 ${formatCount(result.exported)} 张图导出为轮询项目：${result.path}`;
      }
      case "cleanImages": {
        const rename = (["original", "random", "custom"].includes(String(options.rename)) ? String(options.rename) : "original") as ImageFileRenameMode;
        const result = await exportSelectedImages(selection, [], path, rename, rename === "custom" ? String(options.customName ?? "").trim() : null, options.stripMetadata !== false);
        return `已导出 ${formatCount(result.exported)} 张图片到 ${result.directory}${result.missing ? `，${formatCount(result.missing)} 张找不到原图` : ""}`;
      }
      default: {
        const result = await exportImageFiles(selection, path, display.format === "hardlinkImages" ? "hardlink" : "copy");
        return `已导出 ${formatCount(result.exported)} 张图片到 ${result.directory}${result.hardlinkFallbacks ? `，${result.hardlinkFallbacks} 张硬链接失败已改为复制` : ""}${result.missing ? `，${result.missing} 张找不到图片` : ""}`;
      }
    }
  } finally { stop(); setProgress(null); }
}

/**
 * An export the assistant prepared. Nothing is written until the user picks a place here;
 * the outcome is then told to the conversation.
 */
export function ExportCard({ display, conversationId, exported, busy }: {
  display: ExportDisplay;
  conversationId: string | null;
  /** Result recorded in the conversation, if this card was already used. */
  exported: string | null;
  /** The assistant is working; the outcome could not be recorded now. */
  busy: boolean;
}) {
  const view = useImageViewer();
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<ExportProgress | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const done = result ?? exported;
  const ids = display.rowIds ?? [];
  const lines = optionLines(display);
  const start = async () => {
    setRunning(true);
    try {
      const outcome = await runExport(display, setProgress);
      if (outcome === null) return;
      setResult(outcome);
      notify(outcome);
      if (conversationId) await noteAgentExport(conversationId, display.exportId, display.title, outcome).catch(() => {});
    } catch (error) {
      const message = `导出失败：${errorText(error)}`;
      notify(message, "error");
      if (conversationId) await noteAgentExport(conversationId, display.exportId, display.title, message).catch(() => {});
    } finally { setRunning(false); }
  };
  const failed = done?.startsWith("导出失败");
  return <section className={`ag-card${done && !failed ? " is-applied" : ""}`}>
    <header className="ag-card-head">
      {done && !failed ? <Check size={16} /> : <Download size={16} />}
      <strong>{display.title || "导出"}</strong>
      <span className={`ag-badge${done && !failed ? " is-ok" : ""}`}>{done ? (failed ? "导出失败" : "已导出") : "等你选择位置"}</span>
    </header>
    <p className="ag-lede">导出 <b>{formatCount(ids.length)}</b> 张图片 · {FORMAT_LABELS[display.format] ?? display.format}</p>
    {lines.length > 0 && <div className="ag-summary"><ul>{lines.map(line => <li key={line}>{line}</li>)}</ul></div>}
    <ThumbStrip ids={ids} max={12} size={56} onOpen={index => view({ title: display.title || "要导出的图片", ids, index })} />
    {done && <p className={failed ? "ag-error-text" : "ag-op-line"}>{done}</p>}
    {running && progress && progress.total > 0 && <p className="ag-note">正在导出 {formatCount(progress.processed)} / {formatCount(progress.total)}</p>}
    <div className="ag-card-actions">
      <Button variant={done && !failed ? "default" : "primary"} size="sm" disabled={running || busy || !ids.length}
        title={busy ? "助手正在处理，等它停下来再导出" : undefined} onClick={() => void start()}>
        <FolderOutput size={14} />{running ? "导出中…" : done && !failed ? "再导出一份" : "选择位置并导出"}
      </Button>
    </div>
    {!done && <p className="ag-hint">助手不会自己写文件；点按钮后由你选择保存位置</p>}
  </section>;
}
