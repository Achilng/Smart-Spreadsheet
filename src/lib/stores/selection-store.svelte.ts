import { SvelteSet } from "svelte/reactivity";

import {
  countSelectedRows,
  selectedRowIds,
  type DedupeMode,
  type RowSelection,
  type TagMatchMode,
} from "../api";
import { getRow, rowStore } from "./row-store.svelte";
import { setNotice } from "./app-state.svelte";

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
  filteredSingleArtistOnly: false,
  filteredHasVibe: false,
  filteredUntaggedOnly: false,
  filteredSearch: "",
  filteredTotal: 0,
  version: 0,
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

/** 勾选/取消一行；按住 Shift 时从上一次点击位置整段选中。 */
export function toggleRow(rowId: number, index: number, shiftKey: boolean): void {
  if (shiftKey && anchorIndex !== null && anchorIndex !== index) {
    const from = Math.min(anchorIndex, index);
    const to = Math.max(anchorIndex, index);
    let missing = 0;
    for (let cursor = from; cursor <= to; cursor += 1) {
      const row = getRow(cursor);
      if (row) {
        setRowSelected(row.id, true);
      } else {
        missing += 1;
      }
    }
    if (missing > 0) {
      // 范围跨越尚未加载的分页：明确告知实际选中数，避免用户误以为整段已选
      setNotice({
        tone: "error",
        text: `范围内有 ${missing} 行尚未加载，本次只选中了已加载的 ${to - from + 1 - missing} 行。滚动浏览加载后可再次 Shift 选择补全。`,
      });
    }
  } else {
    setRowSelected(rowId, !isRowSelected(rowId));
  }
  anchorIndex = index;
  selection.version += 1;
}

/**
 * 卡片/行主体的修饰键选择（资源管理器惯例）：
 * Ctrl+单击 = 切换该行选中；Shift+单击 = 从锚点整段选中。
 * 返回 true 表示本次点击已作为选择操作消费，调用方不应再当作“查看详情”。
 */
export function modifierSelect(rowId: number, index: number, event: MouseEvent): boolean {
  if (event.shiftKey && anchorIndex !== null) {
    toggleRow(rowId, index, true);
    return true;
  }
  if (event.ctrlKey || event.metaKey) {
    toggleRow(rowId, index, false);
    return true;
  }
  return false;
}

/** 用一组明确行 ID 覆盖当前选择（切换为 explicit 模式）。 */
export function setExplicitSelection(ids: number[]): void {
  selection.kind = "explicit";
  selection.filteredTags = [];
  selection.filteredDedupe = "none";
  selection.filteredSingleArtistOnly = false;
  selection.filteredHasVibe = false;
  selection.filteredUntaggedOnly = false;
  selection.filteredSearch = "";
  selection.filteredTotal = 0;
  selectionIds.clear();
  for (const id of ids) {
    selectionIds.add(id);
  }
  anchorIndex = null;
  selection.version += 1;
}

export function clearSelection(): void {
  selection.kind = "explicit";
  selection.filteredTags = [];
  selection.filteredDedupe = "none";
  selection.filteredSingleArtistOnly = false;
  selection.filteredHasVibe = false;
  selection.filteredUntaggedOnly = false;
  selection.filteredSearch = "";
  selection.filteredTotal = 0;
  selectionIds.clear();
  anchorIndex = null;
  selection.version += 1;
}

/** 排序等重排操作后调用：清掉 Shift 范围选择的锚点，避免按旧顺序选错行。 */
export function resetSelectionAnchor(): void {
  anchorIndex = null;
}

/** 全选当前筛选结果；行数在后端按当前筛选统计。 */
export async function selectAllFiltered(): Promise<number> {
  const dto: RowSelection = {
    kind: "filtered",
    tags: [...rowStore.tags],
    tagMode: rowStore.tagMode,
    dedupe: rowStore.dedupe,
    singleArtistOnly: rowStore.singleArtistOnly,
    hasVibe: rowStore.hasVibe,
    untaggedOnly: rowStore.untaggedOnly,
    search: rowStore.search,
    excludedRowIds: [],
  };
  const requestVersion = selection.version;
  const totalCount = await countSelectedRows(dto);
  // 统计期间筛选或选区已变化：丢弃过期结果，避免“显示旧筛选行数、实际按新筛选执行”。
  if (
    selection.version !== requestVersion ||
    rowStore.tags.length !== dto.tags.length ||
    dto.tags.some((tag, i) => rowStore.tags[i] !== tag) ||
    rowStore.tagMode !== dto.tagMode ||
    rowStore.dedupe !== dto.dedupe ||
    rowStore.singleArtistOnly !== dto.singleArtistOnly ||
    rowStore.hasVibe !== dto.hasVibe ||
    rowStore.untaggedOnly !== dto.untaggedOnly ||
    rowStore.search !== dto.search
  ) {
    return getSelectedCount();
  }
  selection.kind = "filtered";
  selection.filteredTags = dto.tags;
  selection.filteredMode = dto.tagMode;
  selection.filteredDedupe = dto.dedupe;
  selection.filteredSingleArtistOnly = dto.singleArtistOnly;
  selection.filteredHasVibe = dto.hasVibe;
  selection.filteredUntaggedOnly = dto.untaggedOnly;
  selection.filteredSearch = dto.search;
  selection.filteredTotal = totalCount;
  selectionIds.clear();
  anchorIndex = null;
  selection.version += 1;
  return totalCount;
}

/** 将 filtered 全选固定为明确行 ID，供连续批量编辑保持同一目标集合。 */
export async function materializeSelection(): Promise<number> {
  if (selection.kind === "explicit") {
    return selectionIds.size;
  }
  const requestVersion = selection.version;
  const rowIds = await selectedRowIds(selectionDto());
  if (selection.kind !== "filtered" || selection.version !== requestVersion) {
    return getSelectedCount();
  }
  selection.kind = "explicit";
  selection.filteredTags = [];
  selection.filteredDedupe = "none";
  selection.filteredSingleArtistOnly = false;
  selection.filteredHasVibe = false;
  selection.filteredUntaggedOnly = false;
  selection.filteredSearch = "";
  selection.filteredTotal = 0;
  selectionIds.clear();
  for (const rowId of rowIds) {
    selectionIds.add(rowId);
  }
  anchorIndex = null;
  selection.version += 1;
  return rowIds.length;
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
    singleArtistOnly: selection.filteredSingleArtistOnly,
    hasVibe: selection.filteredHasVibe,
    untaggedOnly: selection.filteredUntaggedOnly,
    search: selection.filteredSearch,
    excludedRowIds: [...selectionIds].sort((left, right) => left - right),
  };
}
