import { useState, type ReactElement } from "react";
import { ContextMenu } from "radix-ui";
import type { RowRecord, RowSelection } from "../../lib/api";
import { openCompareWindow } from "../../lib/windows/compare";
import { errorText, formatCount } from "../../lib/utils/format";
import { copyRowPrompt, exportOriginalImage, filterByArtists, requestDelete, requestGroupAssign, requestPromptEdit, requestTagEdit, selectSameArtists, showRowInExplorer } from "../state/row-actions";
import { isSelected, selectedCount, selectionDto, setExplicitSelection, useSelection } from "../state/selection";
import { useTasks } from "../state/tasks";
import { notify } from "../state/notices";

export function RowContextMenu({ row, children }: { row: RowRecord; children: ReactElement }) {
  const busy = useTasks(state => state.busy);
  const [target, setTarget] = useState<{ selection: RowSelection; count: number }>({ selection: { kind: "explicit", rowIds: [row.id] }, count: 1 });
  const hasImage = Boolean(row.imagePath?.trim() || row.storedImagePath?.trim());
  const action = (label: string, onSelect: () => void, disabled = false, danger = false) => <ContextMenu.Item className={`r-menu-item ${danger ? "is-danger" : ""}`} disabled={disabled} onSelect={onSelect}>{label}</ContextMenu.Item>;
  return <ContextMenu.Root onOpenChange={open => {
    if (!open) return;
    if (selectedCount(useSelection.getState()) > 0 && !isSelected(row.id)) setExplicitSelection([row.id]);
    const count = selectedCount(useSelection.getState());
    setTarget(count > 0 ? { selection: selectionDto(), count } : { selection: { kind: "explicit", rowIds: [row.id] }, count: 1 });
  }}><ContextMenu.Trigger asChild>{children}</ContextMenu.Trigger><ContextMenu.Portal>
    <ContextMenu.Content className="r-menu" collisionPadding={8}>
      {action("复制 Prompt", () => void copyRowPrompt(row), !row.positivePrompt?.trim())}
      {action("导出原图", () => void exportOriginalImage(row), !hasImage || busy)}
      {action("在文件管理器中打开", () => void showRowInExplorer(row), !hasImage)}
      <ContextMenu.Separator className="r-menu-separator" />
      {action("对比", () => { void openCompareWindow(row.id).catch(error => notify(`打开对比窗口失败：${errorText(error)}`, "error")); })}
      {action("只看当前画师串", () => filterByArtists(row.artists ?? ""), !row.artists?.trim())}
      {action("选中相同画师串", () => void selectSameArtists(row.artists ?? ""), !row.artists?.trim())}
      <ContextMenu.Separator className="r-menu-separator" />
      {action("编辑 Tag", () => void requestTagEdit(target.selection, target.count), busy)}
      {action("移入分组", () => void requestGroupAssign(target.selection, target.count), busy)}
      {action("编辑提示词", () => void requestPromptEdit(target.selection, target.count), busy)}
      <ContextMenu.Separator className="r-menu-separator" />
      {action(target.count > 1 ? `删除已选 ${formatCount(target.count)} 行` : "删除", () => void requestDelete(target.selection, target.count), busy, true)}
    </ContextMenu.Content>
  </ContextMenu.Portal></ContextMenu.Root>;
}
