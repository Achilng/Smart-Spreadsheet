import type { ReactElement } from "react";
import type { RowRecord } from "../../../lib/api";
import { setCompareSample } from "../../state/compare";
import { copyRowFilePath, requestDelete, showRowInExplorer, useRowActions } from "../../state/row-actions";
import { useTasks } from "../../state/tasks";
import { RightClickMenu } from "../../ui/controls";

export function CompareRowContextMenu({ row, children }: { row: RowRecord; children: ReactElement }) {
  const busy = useTasks(state => state.busy);
  const preparing = useRowActions(state => state.preparing);
  const hasImage = Boolean(row.imagePath?.trim() || row.storedImagePath?.trim());
  return <RightClickMenu items={[
    { label: "复制文件地址", disabled: !hasImage, action: () => void copyRowFilePath(row) },
    { label: "在文件管理器中打开", disabled: !hasImage, action: () => void showRowInExplorer(row) },
    { label: "对比", separator: true, disabled: busy, action: () => void setCompareSample(row.id, true) },
    { label: "删除", separator: true, danger: true, disabled: busy || preparing, action: () => void requestDelete({ kind: "explicit", rowIds: [row.id] }, 1) },
  ]}>{children}</RightClickMenu>;
}
