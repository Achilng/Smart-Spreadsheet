import { getAppSnapshot, getRowsByIds, type AppSnapshot } from "../../lib/api";
import { notifyMainStateChanged, notifyToolboxLibraryChanged } from "../../lib/windows/library-events";
import { hasActiveFilters, refreshCachedRows, refreshTags, reloadRows, useLibrary, useRows } from "./library";
import { clearSelection } from "./selection";
import { invalidateImageCache } from "./image-cache";
import { notify } from "./notices";
import { errorText } from "../../lib/utils/format";

let synchronization = 0;
export interface RowMutationImpact { kind: "tags" | "text"; rowIds: readonly number[] }
/** Synchronize the visible page and detail without changing selection or image caches. */
export async function synchronizeRowsAndDetail(resetScroll?: boolean, impact?: RowMutationImpact): Promise<void> {
  const request = ++synchronization;
  const directory = useLibrary.getState().snapshot?.dataDirectory;
  const activeId = useRows.getState().activeRow?.id;
  const current = () => request === synchronization && directory === useLibrary.getState().snapshot?.dataDirectory;
  let refreshedInPlace = false;
  const refreshRows = async () => {
    const query = useRows.getState().query;
    // Deliberately conservative: filtered, grouped and recently-updated lists retain
    // their existing query semantics; unknown mutations always use the full path.
    if (impact && !resetScroll && query.sort !== "recentlyUpdated" && !query.groupView && !hasActiveFilters(query)) {
      refreshedInPlace = await refreshCachedRows(impact.rowIds, current);
    }
    if (!refreshedInPlace && current()) await reloadRows({ resetScroll, keepActive: true });
  };
  const refreshTagCounts = impact?.kind !== "text";
  await Promise.all([refreshTagCounts ? refreshTags() : Promise.resolve(), refreshRows()]);
  if (!current()) return;
  if (!refreshedInPlace && activeId !== undefined && useRows.getState().activeRow?.id === activeId) {
    const rows = await getRowsByIds([activeId]);
    if (current() && useRows.getState().activeRow?.id === activeId) useRows.setState({ activeRow: rows[0] ?? null });
  }
  const error = (refreshTagCounts ? useLibrary.getState().tagError : null) ?? useRows.getState().error;
  if (error) throw new Error(error);
}

/** A committed mutation must never look like a failed save or be replayed by undo. */
export async function afterLibraryMutation(options: {
  origin?: "main" | "toolbox"; resetScroll?: boolean;
  impact?: RowMutationImpact;
  history?: () => Promise<unknown>; afterRefresh?: () => Promise<unknown>;
} = {}): Promise<void> {
  const directory = useLibrary.getState().snapshot?.dataDirectory;
  const step = async (label: string, action: () => Promise<unknown>) => {
    try { await action(); }
    catch (error) { notify(`操作已完成，但${label}失败：${errorText(error)}`, "error"); }
  };
  if (options.history) await step("记录撤销历史", options.history);
  if (directory !== useLibrary.getState().snapshot?.dataDirectory) return;
  await step("刷新界面", () => synchronizeRowsAndDetail(options.resetScroll, options.impact));
  if (directory !== useLibrary.getState().snapshot?.dataDirectory) return;
  if (options.afterRefresh) await step("同步关联视图", options.afterRefresh);
  const windowOrigin = new URLSearchParams(window.location.search).get("window") === "toolbox" ? "toolbox" : "main";
  if (windowOrigin === "toolbox") await notifyMainStateChanged("libraryEdited");
  notifyToolboxLibraryChanged(options.origin ?? windowOrigin);
}

export async function refreshLibrary(options: { snapshot?: AppSnapshot; resetScroll?: boolean; preserveSelection?: boolean; origin?: "main" | "toolbox" } = {}): Promise<void> {
  useLibrary.setState({ snapshot: options.snapshot ?? await getAppSnapshot() });
  if (!options.preserveSelection) { clearSelection(); invalidateImageCache(); }
  await afterLibraryMutation(options);
}
