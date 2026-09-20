import { create } from "zustand";
import { countSelectedRows, type RowSelection } from "../../lib/api";
import { snapshotQueryFilters } from "../../lib/utils/library-query";
import { rowAt, useRows } from "./library";
import { notify } from "./notices";

type FilteredSelection = Extract<RowSelection, { kind: "filtered" }>;
interface SelectionState {
  ids: ReadonlySet<number>;
  filtered: FilteredSelection | null;
  total: number;
  anchor: number | null;
  version: number;
}
export const useSelection = create<SelectionState>(() => ({ ids: new Set(), filtered: null, total: 0, anchor: null, version: 0 }));
export const selectedCount = (state: SelectionState): number => state.filtered ? Math.max(0, state.total - state.ids.size) : state.ids.size;

export function clearSelection(): void {
  useSelection.setState(state => ({ ids: new Set(), filtered: null, total: 0, anchor: null, version: state.version + 1 }));
}

export function isSelected(id: number, state = useSelection.getState()): boolean {
  return state.filtered ? !state.ids.has(id) : state.ids.has(id);
}

export function toggleRow(id: number, index: number, shift: boolean): void {
  const state = useSelection.getState();
  const ids = new Set(state.ids);
  const set = (rowId: number, selected: boolean) => {
    if (selected !== Boolean(state.filtered)) ids.add(rowId); else ids.delete(rowId);
  };
  if (shift && state.anchor !== null && state.anchor !== index) {
    const from = Math.min(state.anchor, index), to = Math.max(state.anchor, index);
    let missing = 0;
    for (let cursor = from; cursor <= to; cursor++) {
      const row = rowAt(cursor);
      if (row) set(row.id, true); else missing++;
    }
    if (missing) notify(`范围内有 ${missing} 行尚未加载，本次只选中了已加载的 ${to - from + 1 - missing} 行。滚动浏览加载后可再次 Shift 选择补全。`, "error");
  } else set(id, !isSelected(id, state));
  useSelection.setState({ ids, anchor: index, version: state.version + 1 });
}

export function selectionDto(): RowSelection {
  const state = useSelection.getState();
  const ids = [...state.ids].sort((a, b) => a - b);
  return state.filtered ? { ...structuredClone(state.filtered), excludedRowIds: ids } : { kind: "explicit", rowIds: ids };
}

export async function selectAllFiltered(): Promise<void> {
  const state = useSelection.getState();
  const query = snapshotQueryFilters(useRows.getState().query);
  const filtered: FilteredSelection = { kind: "filtered", ...query, excludedRowIds: [] };
  const total = await countSelectedRows(filtered);
  if (state.version !== useSelection.getState().version || JSON.stringify(query) !== JSON.stringify(snapshotQueryFilters(useRows.getState().query))) return;
  useSelection.setState({ filtered, ids: new Set(), total, anchor: null, version: state.version + 1 });
}

// Pure sort changes retain selected IDs but invalidate the range anchor.
useRows.subscribe((state, previous) => {
  if (state.query === previous.query) return;
  const { sort: currentSort, ...currentQuery } = state.query;
  const { sort: previousSort, ...previousQuery } = previous.query;
  if (JSON.stringify(currentQuery) !== JSON.stringify(previousQuery)) clearSelection();
  else if (currentSort !== previousSort) useSelection.setState({ anchor: null });
});
