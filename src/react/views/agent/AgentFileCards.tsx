import { useState } from "react";
import { Archive, Check, FolderInput, FolderOpen, MonitorUp, Trash2 } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { agentCardRequest } from "../../../lib/api/agent";
import { errorText, formatCount } from "../../../lib/utils/format";
import { Button } from "../../ui/controls";
import { notify } from "../../state/notices";
import { ThumbStrip, useImageViewer } from "./AgentMedia";

export interface DeleteDisplay { cardId: string; title: string; rowIds: number[] }
export interface ImportDisplay { cardId: string; title: string; source: "folder" | "archive" | string }

/**
 * Images the assistant suggests deleting. Nothing is deleted here: the button opens the main
 * window's usual delete dialog, where the user decides about the original files and confirms.
 */
export function DeleteCard({ display, conversationId, result, busy }: {
  display: DeleteDisplay; conversationId: string | null;
  /** How the card ended, once the main window reported it. */
  result: string | null; busy: boolean;
}) {
  const view = useImageViewer();
  const [sent, setSent] = useState(false);
  const ids = display.rowIds ?? [];
  const deleted = !!result && result.startsWith("已删除");
  const request = async () => {
    if (!conversationId) return;
    try { await agentCardRequest(conversationId, display.cardId, "delete", display.title, { rowIds: ids }); setSent(true); }
    catch (error) { notify(`没能打开删除确认：${errorText(error)}`, "error"); }
  };
  return <section className={`ag-card${deleted ? " is-applied" : ""}`}>
    <header className="ag-card-head">
      {deleted ? <Check size={16} /> : <Trash2 size={16} />}
      <strong>{display.title || "删除图片"}</strong>
      <span className={`ag-badge${deleted ? " is-ok" : ""}`}>{result ? (deleted ? "已删除" : "没有删除") : "等你确认"}</span>
    </header>
    <p className="ag-lede">准备删除 <b>{formatCount(ids.length)}</b> 张图片</p>
    <ThumbStrip ids={ids} max={12} size={56} onOpen={index => view({ title: display.title || "要删除的图片", ids, index })} />
    {result && <p className="ag-op-line">{result}</p>}
    {!result && <div className="ag-card-actions">
      <Button variant="primary" size="sm" disabled={busy || !conversationId || !ids.length} title={busy ? "助手正在处理，等它停下来再操作" : undefined} onClick={() => void request()}>
        <MonitorUp size={14} />{sent ? "再次打开删除确认" : "在主窗口确认删除"}
      </Button>
    </div>}
    {!result && <p className="ag-hint">点按钮后主窗口会弹出平时的删除确认框，可以选择是否把原图移入回收站。删除后不能在这里撤销</p>}
  </section>;
}

/** An import the assistant prepared; the user picks the folder or archive here. */
export function ImportCard({ display, conversationId, result, busy }: {
  display: ImportDisplay; conversationId: string | null; result: string | null; busy: boolean;
}) {
  const [running, setRunning] = useState(false);
  const imported = !!result && result.startsWith("已导入");
  const choose = async (source: "folder" | "archive") => {
    if (!conversationId) return;
    setRunning(true);
    try {
      const path = await open(source === "folder"
        ? { directory: true, multiple: false, title: "选择要导入的图片文件夹（追加进资料库）" }
        : { directory: false, multiple: false, title: "选择要导入的压缩包（追加进资料库）", filters: [{ name: "压缩包", extensions: ["zip", "7z", "rar"] }] });
      if (typeof path !== "string") return;
      await agentCardRequest(conversationId, display.cardId, "import", display.title, { path });
      notify("已交给主窗口导入，进度显示在主窗口顶部");
    } catch (error) { notify(`没能开始导入：${errorText(error)}`, "error"); }
    finally { setRunning(false); }
  };
  const archive = display.source === "archive";
  const disabled = running || busy || !conversationId;
  const title = busy ? "助手正在处理，等它停下来再操作" : undefined;
  return <section className={`ag-card${imported ? " is-applied" : ""}`}>
    <header className="ag-card-head">
      {imported ? <Check size={16} /> : <FolderInput size={16} />}
      <strong>{display.title || "导入图片"}</strong>
      <span className={`ag-badge${imported ? " is-ok" : ""}`}>{result ? (imported ? "已导入" : "没有新图") : "等你选择"}</span>
    </header>
    <p className="ag-lede">把一个{archive ? "压缩包（zip / 7z / rar）" : "文件夹"}里的图片追加进资料库</p>
    {result && <p className="ag-op-line">{result}</p>}
    <div className="ag-card-actions">
      <Button variant={archive || imported ? "default" : "primary"} size="sm" disabled={disabled} title={title} onClick={() => void choose("folder")}><FolderOpen size={14} />选择文件夹</Button>
      <Button variant={archive && !imported ? "primary" : "default"} size="sm" disabled={disabled} title={title} onClick={() => void choose("archive")}><Archive size={14} />选择压缩包</Button>
    </div>
    <p className="ag-hint">已在库里的图片会跳过；刚导入的一批可以在主窗口按 Ctrl+Z 撤销</p>
  </section>;
}
