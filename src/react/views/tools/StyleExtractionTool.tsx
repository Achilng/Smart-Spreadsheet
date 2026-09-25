import { useEffect, useRef, useState } from "react";
import { emitTo, listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { applyStyleChanges, exportStyleRequest, previewStyleResult, type StylePreview, type StyleExportSummary } from "../../../lib/api/style-extraction";
import type { ToolboxSelectionSnapshot } from "../../../lib/windows/toolbox";
import { notifyMainStateChanged } from "../../../lib/windows/library-events";
import { errorText } from "../../../lib/utils/format";
import { useTasks, runTask } from "../../state/tasks";
import { recordHistory, useHistory } from "../../state/history";
import { notify } from "../../state/notices";
import { Button, Checkbox } from "../../ui/controls";
import { ToolPage, ToolCard, ToolError, ToolMetrics } from "./shared";
import { StyleWebTools } from "./StyleWebTools";

export default function StyleExtractionTool({ active }: { active: boolean }) {
  const [selection, setSelection] = useState<ToolboxSelectionSnapshot | null>(null);
  const [scope, setScope] = useState("all"), [include, setInclude] = useState(false), [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<StylePreview | null>(null), [exported, setExported] = useState<StyleExportSummary | null>(null), [page, setPage] = useState(0);
  const busy = useTasks(s => s.busy), historyBusy = useHistory(s => s.busy), pending = useRef(false);
  const disabled = busy || historyBusy;
  useEffect(() => {
    let disposed = false; let stop: (() => void) | undefined;
    void listen<ToolboxSelectionSnapshot>("main://selection-changed", event => { if (!disposed) setSelection(event.payload); }).then(fn => { if (disposed) fn(); else { stop = fn; void emitTo("main", "toolbox://request-selection"); } }).catch(e => setError(errorText(e)));
    return () => { disposed = true; stop?.(); };
  }, []);
  useEffect(() => { if (active) void emitTo("main", "toolbox://request-selection").catch(() => {}); }, [active]);
  async function perform(label: string, fn: () => Promise<void>) { if (disabled || pending.current) return; pending.current = true; setError(null); try { await runTask(label, fn); } catch (e) { setError(errorText(e)); } finally { pending.current = false; } }
  async function exportFile() {
    await perform("导出 LLM 待处理文本", async () => {
      const target = scope === "all" ? null : scope === "filtered" ? selection?.filteredSelection : selection?.selection;
      if (scope !== "all" && !target) throw Error("尚未获取主窗口范围，请返回主窗口后重试。");
      const path = await save({ title: "导出画风提取待处理文本", defaultPath: "画风待处理.json", filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!path) return;
      const result = await exportStyleRequest(target ?? null, include, path); setExported(result); notify(`已导出 ${result.uniquePrompts} 段不同正文。`);
    });
  }
  async function importFile() {
    await perform("预览 LLM 处理结果", async () => {
      const path = await open({ title: "选择画风提取结果 JSON", multiple: false, filters: [{ name: "JSON", extensions: ["json"] }] });
      if (typeof path !== "string") return;
      setPreview(null); const result = await previewStyleResult(path); setPreview(result); setPage(0);
    });
  }
  async function apply() {
    if (!preview?.changes.length) return;
    const value = preview;
    await perform("应用 LLM 画风结果", async () => {
      const result = await applyStyleChanges(value.libraryId, value.changes);
      if (result.changes.length) {
        const restore = async (reverse: boolean) => { await applyStyleChanges(value.libraryId, result.changes, reverse, true); setPreview(null); await notifyMainStateChanged("libraryEdited"); };
        recordHistory({ label: `LLM 画风提取（${result.changes.length} 条）`, undo: () => restore(true), redo: () => restore(false) });
      }
      setPreview(null); await notifyMainStateChanged("libraryEdited"); notify(`已更新 ${result.changes.length} 条记录${result.conflicts ? `，${result.conflicts} 条预览后发生变化，已跳过` : ""}。`);
    });
  }
  return <ToolPage><ToolCard><h3>1 · 导出正向提示词</h3><p>原文完全一致才合并。外部工具处理后，将结果带回这里。</p><div className="rt-fields"><label>导出范围 <select value={scope} disabled={disabled} onChange={e => setScope(e.target.value)}><option value="all">全部记录</option><option value="selected">主窗口选中记录（{selection?.count ?? 0} 条）</option><option value="filtered">主窗口当前筛选结果</option></select></label><label><Checkbox checked={include} disabled={disabled} onCheckedChange={v => setInclude(v === true)} />包含已由 LLM 处理的记录</label></div><div className="rt-actions"><Button variant="primary" disabled={disabled || (scope === "selected" && !selection?.count)} onClick={() => void exportFile()}>导出待处理 JSON</Button></div>{exported && <ToolMetrics items={[[exported.rows, "范围内记录"], [exported.skipped, "已处理或空正文"], [exported.uniquePrompts, "去重后正文"]]} />}</ToolCard>
    <ToolCard><h3>2 · 提取与批改</h3><p>点击下方按钮，在默认浏览器打开服务器工作台。提取完成后下载结果 JSON；需要人工检查时，在批改台载入结果。</p><StyleWebTools /><p>提取任务在服务器后台运行，关闭网页或退出智能表格不影响处理；再次打开即可查看进度。批改记录也会同步保存到服务器。</p></ToolCard>
    <ToolCard><div className="rt-row"><div><h3>3 · 导入并预览结果</h3><p>按完整正向原文匹配当前资料库所有记录。同一正文的图片一起更新，成功应用后显示 LLM 标记。</p></div><Button disabled={disabled} onClick={() => void importFile()}>选择结果 JSON</Button></div></ToolCard><ToolError error={error} />
    {preview && <ToolCard><ToolMetrics items={[[preview.changes.length, "待更新记录"], [preview.emptyResults, "匹配的无画风结果"], [preview.unchanged, "结果未变化"], [preview.failed + preview.invalidItems, "失败或异常项"], [preview.unmatched, "正文无匹配"]]} /><p>其中 {preview.changes.filter(x => x.oldSource).length} 条将覆盖已有 LLM 结果。无画风结果会清空旧画师串；失败项保持原样。</p>{preview.issues.length > 0 && <details><summary>查看异常原因</summary>{preview.issues.map((issue, i) => <p key={i} style={{ overflowWrap: "anywhere" }}>{issue}</p>)}</details>}
      {preview.changes.slice(page * 20, page * 20 + 20).map(change => <section key={change.rowId} style={{ borderTop: "1px solid #e5e7eb", padding: "14px 0" }}><strong>记录 #{change.rowId}</strong><div className="rt-columns"><div><small>原画师串</small><p style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere", maxHeight: 150, overflow: "auto" }}>{change.oldArtists || "空"}</p></div><div><small>LLM 结果</small><p style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere", maxHeight: 150, overflow: "auto" }}>{change.newArtists || "未识别到画风"}</p></div></div></section>)}
      <div className="rt-row"><div className="rt-actions"><Button disabled={!page || disabled} onClick={() => setPage(p => p - 1)}>上一页</Button><small>{page + 1} / {Math.max(1, Math.ceil(preview.changes.length / 20))}</small><Button disabled={(page + 1) * 20 >= preview.changes.length || disabled} onClick={() => setPage(p => p + 1)}>下一页</Button></div><Button variant="primary" disabled={disabled || !preview.changes.length} onClick={() => void apply()}>应用 {preview.changes.length} 条结果</Button></div></ToolCard>}
  </ToolPage>;
}
