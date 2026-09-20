import { useState } from "react";
import { Upload } from "lucide-react";
import { findReplacePrompt, prefixArtistTag, type RowSelection } from "../../lib/api";
import { errorText, formatCount } from "../../lib/utils/format";
import { closeRowAction, confirmDelete, useRowActions } from "../state/row-actions";
import { cancelDropImport, confirmDropImport, useDropImport } from "../state/drop-import";
import { captureSelectionStates, recordRowStateChange } from "../state/history";
import { refreshLibrary } from "../state/library-changes";
import { useLibrary } from "../state/library";
import { useWorkspace } from "../state/workspace";
import { runTask, useTasks } from "../state/tasks";
import { notify } from "../state/notices";
import { Button, Checkbox, Input, Modal } from "../ui/controls";
import { TagAssignDialog } from "./TagAssignDialog";
import { GroupAssignDialog } from "./groups/GroupAssignDialog";
import "./row-actions.css";

export function RowActionDialogs() {
  const state = useRowActions();
  const request = state.request;
  return <>
    {request?.kind === "tags" && <TagAssignDialog key={request.serial} selection={request.selection} count={request.count} onClose={closeRowAction} />}
    {request?.kind === "group" && <GroupAssignDialog key={request.serial} open selection={request.selection} onOpenChange={open => { if (!open) closeRowAction(); }} />}
    {request?.kind === "prompt" && <PromptEditDialog key={request.serial} selection={request.selection} count={request.count} onClose={closeRowAction} />}
    <Modal open={request?.kind === "delete"} title={`删除 ${formatCount(request?.count ?? 0)} 行？`} description="对应的 Tag 关联、分组关系、受管图片副本和缩略图缓存也会永久清理。" onClose={closeRowAction} busy={state.deleting}
      footer={<><Button disabled={state.deleting} onClick={closeRowAction}>取消</Button><Button variant="danger" disabled={state.deleting} onClick={() => void confirmDelete()}>{state.deleting ? "正在删除…" : "确认删除"}</Button></>}>
      <p className="r-row-warning">此操作不会加入撤销记录，删除后无法通过 Ctrl+Z 恢复，并会清空当前撤销/重做记录。</p>
      <label className="r-row-trash-option"><Checkbox checked={state.trashOriginals} disabled={state.deleting} onCheckedChange={checked => useRowActions.setState({ trashOriginals: checked === true })} /><span><strong>同时将原始图片文件移入回收站</strong><small>默认勾选。原图移入 Windows 回收站后，应用内无法恢复；压缩包来源没有独立原文件，会自动跳过。</small></span></label>
      {state.error && <p role="alert" className="r-field-error">{state.error}</p>}
    </Modal>
    <DropImportDialog />
  </>;
}

export function PromptEditDialog({ selection, count, onClose }: { selection: RowSelection; count: number; onClose: () => void }) {
  const [tab, setTab] = useState<"replace" | "artist">("replace");
  const [find, setFind] = useState("");
  const [replacement, setReplacement] = useState("");
  const [artist, setArtist] = useState("");
  const [busy, setBusy] = useState(false);
  const taskBusy = useTasks(state => state.busy);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  async function apply(): Promise<void> {
    if (busy || taskBusy || !(tab === "replace" ? find.trim() : artist.trim())) return;
    setBusy(true); setError(null); setResult(null);
    try {
      await runTask(tab === "replace" ? "批量替换提示词" : "批量修正画师前缀", async () => {
        const before = await captureSelectionStates(selection);
        const response = tab === "replace" ? await findReplacePrompt(selection, find, replacement) : await prefixArtistTag(selection, artist.trim());
        setResult(tab === "replace" ? `已替换 ${formatCount(response.affectedRows)} 行` : `已修正 ${formatCount(response.affectedRows)} 行画师前缀`);
        try { await recordRowStateChange(tab === "replace" ? "批量替换提示词" : "批量修正画师前缀", before); }
        catch (historyError) { notify(`编辑已完成，但未能记录撤销历史：${errorText(historyError)}`, "error"); }
        try { await refreshLibrary({ preserveSelection: true }); }
        catch (refreshError) { notify(`编辑已完成，但刷新失败：${errorText(refreshError)}`, "error"); }
      });
    } catch (failure) { setError(errorText(failure)); }
    finally { setBusy(false); }
  }
  const disabled = busy || taskBusy;
  return <Modal open title={`编辑提示词（${formatCount(count)} 行）`} onClose={onClose} busy={busy} width={440}
    footer={<><Button disabled={busy} onClick={onClose}>关闭</Button><Button variant="primary" disabled={disabled || !(tab === "replace" ? find.trim() : artist.trim())} onClick={() => void apply()}>{busy ? "执行中…" : tab === "replace" ? "执行替换" : "修正前缀"}</Button></>}>
    <div className="r-row-edit-tabs" role="tablist" aria-label="提示词编辑方式"><Button role="tab" aria-selected={tab === "replace"} disabled={busy} onClick={() => { setTab("replace"); setError(null); setResult(null); }}>查找替换</Button><Button role="tab" aria-selected={tab === "artist"} disabled={busy} onClick={() => { setTab("artist"); setError(null); setResult(null); }}>修正画师前缀</Button></div>
    <div className="r-row-edit-fields" role="tabpanel">{tab === "replace" ? <>
      <label>查找<Input value={find} onChange={event => setFind(event.target.value)} placeholder="要替换的文本…" disabled={disabled} /></label>
      <label>替换为<Input value={replacement} onChange={event => setReplacement(event.target.value)} placeholder="替换后的文本（可为空）…" disabled={disabled} /></label>
    </> : <>
      <label>画师名（不带 artist:）<Input value={artist} onChange={event => setArtist(event.target.value)} placeholder="例如 parsley_f" disabled={disabled} onKeyDown={event => { if (event.key === "Enter" && !event.nativeEvent.isComposing) { event.preventDefault(); void apply(); } }} /></label>
      <p className="r-muted">只修正选中图片中完全匹配的该画师 tag；已带 artist: 的会跳过，支持 (tag:1.2)、花/方括号权重、0.7::tag 等格式。</p>
    </>}</div>
    {result && <p role="status" className="r-success">{result}</p>}{error && <p role="alert" className="r-field-error">{error}</p>}
  </Modal>;
}

export function DropImportDialog() {
  const state = useDropImport();
  const autoPrefix = useLibrary(store => store.snapshot?.autoArtistPrefixOnImport);
  const materials = useWorkspace(store => store.viewMode === "materials");
  return <><Modal open={state.open} onClose={cancelDropImport} busy={state.busy} title={`确定要导入以下 ${formatCount(state.paths.length)} 个项目吗？`} description="将追加导入到当前资料库。" width={480}
    footer={<><Button disabled={state.busy} onClick={cancelDropImport}>取消</Button><Button variant="primary" disabled={state.busy} onClick={() => void confirmDropImport()}>{state.busy ? state.paths.length > 1 ? `正在导入第 ${state.currentIndex}/${state.paths.length} 个…` : "导入中…" : "确认导入"}</Button></>}>
    {autoPrefix && <p className="r-row-prefix-hint">已开启自动画师前缀检查：导入完成后直接处理，不会再要求确认。</p>}
    {!!state.ignoredCount && <p className="r-field-error">另有 {formatCount(state.ignoredCount)} 个不支持的文件已被忽略（只支持 PNG、文件夹和 zip / 7z / rar）。</p>}
    <ul className="r-drop-paths">{state.paths.map(path => <li key={path} title={path}>{path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? path}</li>)}</ul>
  </Modal>{state.dragging && <div className="r-native-drop-overlay" aria-live="polite"><Upload size={34} /><strong>{materials ? "松开以导入素材" : "松开以导入文件"}</strong><span>{materials ? "图片文件或文件夹" : "PNG 图片、文件夹或压缩包"}</span></div>}</>;
}
