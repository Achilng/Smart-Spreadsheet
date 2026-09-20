import { useState } from "react";
import { clearSelection, selectedCount, selectionDto, useSelection } from "../state/selection";
import { selectAllCurrentView } from "../state/view-selection";
import { requestDelete, requestGroupAssign, requestPromptEdit, requestTagEdit, useRowActions } from "../state/row-actions";
import { buildExportItems } from "../state/export-actions";
import { useTasks } from "../state/tasks";
import { notify } from "../state/notices";
import { errorText } from "../../lib/utils/format";
import { Button, Menu } from "../ui/controls";

export function SelectionBar() {
  const count = useSelection(selectedCount), busy = useTasks(state => state.busy), preparing = useRowActions(state => state.preparing);
  const [selecting, setSelecting] = useState(false);
  if (!count) return null;
  const disabled = busy || preparing || selecting;
  return <div className="r-selection-bar"><strong>已选择 {count.toLocaleString()} 张</strong><div>
    <Button variant="ghost" disabled={selecting} onClick={clearSelection}>取消选择</Button>
    <Button disabled={disabled} onClick={async () => { setSelecting(true); try { await selectAllCurrentView(); } catch (error) { notify(`全选失败：${errorText(error)}`, "error"); } finally { setSelecting(false); } }}>{selecting ? "选择中…" : "全选当前视图"}</Button>
    <Button disabled={disabled} onClick={() => void requestTagEdit(selectionDto(), count)}>编辑 Tag</Button>
    <Menu label="更多操作" disabled={disabled} direction="up" items={[
      { label: "移入分组", action: () => void requestGroupAssign(selectionDto(), count) },
      { label: "编辑提示词", action: () => void requestPromptEdit(selectionDto(), count) },
      { label: "删除所选", danger: true, separator: true, action: () => void requestDelete(selectionDto(), count) },
    ]} />
    <Menu label="导出所选" direction="up" variant="primary" disabled={disabled} items={buildExportItems()} />
  </div></div>;
}
