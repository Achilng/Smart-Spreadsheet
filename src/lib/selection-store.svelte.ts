import { SvelteSet } from "svelte/reactivity";

import {
  countSelectedRows,
  type DedupeMode,
  type RowSelection,
  type TagMatchMode,
} from "../api";
import { getRow, rowStore } from "./row-store.svelte";

/**
 * 选择模型，沿用后端的 explicit / filtered 双模式：
 * - explicit：selectionIds 保存已勾选的行 ID
 * - filtered：表示“全选当前筛选结果”，selectionIds 保存被排除的行 ID
 */
export const selection = $state({
  kind: "explicit" as "explicit" | "filtered",
  filteredTags: [] as string[],
  filteredMode: "and" as TagMatchMode,
  filteredDedupe: "none" as DedupeMode,
  filteredTotal: 0,
});

export const selectionIds = new SvelteSet<number>();

let anchorIndex: number | null = null;

export function isRowSelected(rowId: number): boolean {
  return selection.kind === "explicit" ? selectionIds.has(rowId) : !selectionIds.has(rowId);
}

export function getSelectedCount(): number {
  if (selection.kind === "explicit") {
    return selectionIds.size;
  }
  return Math.max(0, selection.filteredTotal - selectionIds.size);
}

function setRowSelected(rowId: number, selected: boolean): void {
  if (selection.kind === "explicit") {
    if (selected) {
      selectionIds.add(rowId);
    } else {
      selectionIds.delete(rowId);
    }
  } else if (selected) {
    selectionIds.delete(rowId);
  } else {
    selectionIds.add(rowId);
  }
}

/** 勾选/取消一行；按住 Shift 时从上一次点击位置整段选中（仅覆盖已加载的行）。 */
export function toggleRow(rowId: number, index: number, shiftKey: boolean): void {
  if (shiftKey && anchorIndex !== null && anchorIndex !== index) {
    const from = Math.min(anchorIndex, index);
    const to = Math.max(anchorIndex, index);
    for (let cursor = from; cursor <= to; cursor += 1) {
      const row = getRow(cursor);
      if (row) {
        setRowSelected(row.id, true);
      }
    }
  } else {
    setRowSelected(rowId, !isRowSelected(rowId));
  }
  anchorIndex = index;
}

export function clearSelection(): void {
  selection.kind = "explicit";
  selection.filteredTags = [];
  selection.filteredDedupe = "none";
  selection.filteredTotal = 0;
  selectionIds.clear();
  anchorIndex = null;
}

/** 全选当前筛选结果；行数在后端按当前筛选统计。 */
export async function selectAllFiltered(): Promise<number> {
  const dto: RowSelection = {
    kind: "filtered",
    tags: [...rowStore.tags],
    tagMode: rowStore.tagMode,
    dedupe: rowStore.dedupe,
    excludedRowIds: [],
  };
  const totalCount = await countSelectedRows(dto);
  selection.kind = "filtered";
  selection.filteredTags = [...rowStore.tags];
  selection.filteredMode = rowStore.tagMode;
  selection.filteredDedupe = rowStore.dedupe;
  selection.filteredTotal = totalCount;
  selectionIds.clear();
  anchorIndex = null;
  return totalCount;
}

export function selectionDto(): RowSelection {
  if (selection.kind === "explicit") {
    return {
      kind: "explicit",
      rowIds: [...selectionIds].sort((left, right) => left - right),
    };
  }
  return {
    kind: "filtered",
    tags: [...selection.filteredTags],
    tagMode: selection.filteredMode,
    dedupe: selection.filteredDedupe,
    excludedRowIds: [...selectionIds].sort((left, right) => left - right),
  };
}
