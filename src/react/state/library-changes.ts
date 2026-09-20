import { getAppSnapshot, getRowsByIds, type AppSnapshot } from "../../lib/api";
import { notifyToolboxLibraryChanged } from "../../lib/windows/library-events";
import { refreshTags, reloadRows, useLibrary, useRows } from "./library";
import { clearSelection } from "./selection";
import { invalidateImageCache } from "./image-cache";

export async function refreshLibrary(options: { snapshot?: AppSnapshot; resetScroll?: boolean; preserveSelection?: boolean; origin?: "main" | "toolbox" } = {}): Promise<void> {
  const activeId = useRows.getState().activeRow?.id;
  useLibrary.setState({ snapshot: options.snapshot ?? await getAppSnapshot() });
  if (!options.preserveSelection) clearSelection();
  if (!options.preserveSelection) invalidateImageCache();
  await Promise.all([refreshTags(), reloadRows({ resetScroll: options.resetScroll, keepActive: true })]);
  if (activeId !== undefined) {
    const rows = await getRowsByIds([activeId]);
    if (useRows.getState().activeRow?.id === activeId) useRows.setState({ activeRow: rows[0] ?? null });
  }
  notifyToolboxLibraryChanged(options.origin ?? "main");
}
