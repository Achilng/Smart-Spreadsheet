import type { RowRecord } from "../api";
import {
  getSelectedCount,
  isRowSelected,
  setExplicitSelection,
} from "./selection-store.svelte";

export const contextMenu = $state({
  open: false,
  x: 0,
  y: 0,
  row: null as RowRecord | null,
});

export function showContextMenu(row: RowRecord, x: number, y: number): void {
  // 资源管理器惯例：右键一个不在选区内的行时，选区收缩为该行，
  // 保证菜单里的批量操作（删除已选 N 行）与用户眼前的目标一致。
  if (getSelectedCount() > 0 && !isRowSelected(row.id)) {
    setExplicitSelection([row.id]);
  }
  contextMenu.row = row;
  contextMenu.x = x;
  contextMenu.y = y;
  contextMenu.open = true;
}

export function hideContextMenu(): void {
  contextMenu.open = false;
}
