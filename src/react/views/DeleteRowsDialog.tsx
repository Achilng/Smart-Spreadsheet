import { formatCount } from "../../lib/utils/format";
import { closeRowAction, confirmDelete, useRowActions, type AfterRowsDeleted } from "../state/row-actions";
import { Button, Checkbox, Modal } from "../ui/controls";
import "./row-actions.css";

/** Shared confirmation and deletion behavior for the main and comparison windows. */
export function DeleteRowsDialog({ afterDelete }: { afterDelete?: AfterRowsDeleted }) {
  const state = useRowActions();
  const request = state.request;
  return <Modal open={request?.kind === "delete"} title={`删除 ${formatCount(request?.count ?? 0)} 行？`} description="对应的 Tag 关联、分组关系、受管图片副本和缩略图缓存也会永久清理。" onClose={closeRowAction} busy={state.deleting}
    footer={<><Button disabled={state.deleting} onClick={closeRowAction}>取消</Button><Button variant="danger" disabled={state.deleting} onClick={() => void confirmDelete(afterDelete)}>{state.deleting ? "正在删除…" : "确认删除"}</Button></>}>
    <p className="r-row-warning">此操作不会加入撤销记录，删除后无法通过 Ctrl+Z 恢复，并会清空当前撤销/重做记录。</p>
    <label className="r-row-trash-option"><Checkbox checked={state.trashOriginals} disabled={state.deleting} onCheckedChange={checked => useRowActions.setState({ trashOriginals: checked === true })} /><span><strong>同时将原始图片文件移入回收站</strong><small>默认勾选。原图移入 Windows 回收站后，应用内无法恢复；压缩包来源没有独立原文件，会自动跳过。</small></span></label>
    {state.error && <p role="alert" className="r-field-error">{state.error}</p>}
  </Modal>;
}
