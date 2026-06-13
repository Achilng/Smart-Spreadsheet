import type { RowRecord } from "../api";

export const contextMenu = $state({
  open: false,
  x: 0,
  y: 0,
  row: null as RowRecord | null,
});

export function showContextMenu(row: RowRecord, x: number, y: number): void {
  contextMenu.row = row;
  contextMenu.x = x;
  contextMenu.y = y;
  contextMenu.open = true;
}

export function hideContextMenu(): void {
  contextMenu.open = false;
}
