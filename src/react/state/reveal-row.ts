import { getRowIndex, getRowsByIds } from "../../lib/api";
import { flushSync } from "react-dom";
import { defaultFilters, reloadRows, useLibrary, useRows } from "./library";
import { useWorkspace } from "./workspace";
import { clearSelection } from "./selection";
import { captureScrollSnapshot } from "../../lib/stores/view-state";
import { galleryCellPosition, galleryLayout } from "../../lib/images/gallery-layout";

export async function revealRow(rowId: number): Promise<void> {
  const directory = useLibrary.getState().snapshot?.dataDirectory;
  const sort = useRows.getState().query.sort;
  const [rows, index] = await Promise.all([getRowsByIds([rowId]), getRowIndex(rowId, sort)]);
  if (directory !== useLibrary.getState().snapshot?.dataDirectory) return;
  if (!rows[0] || index < 0) throw new Error("图片记录已不存在");
  // Measure after the gallery and its detail panel have taken their final width.
  flushSync(() => useWorkspace.setState({ viewMode: "gallery", detailOpen: true }));
  // The gallery excludes its reserved scrollbar gutter; the main area's width
  // accumulates a positioning error on every row when jumping deep into a library.
  const width = document.querySelector(".r-gallery")?.clientWidth ?? 600;
  const top = galleryCellPosition(index, galleryLayout(width, useWorkspace.getState().galleryCardSize, Math.max(index + 1, useRows.getState().total))).y;
  const scroll = captureScrollSnapshot();
  scroll.positions = [["gallery", Math.max(0, top - 16)]];
  scroll.ranges = [["gallery", { first: index, last: index }]];
  scroll.unfilteredPositions = null; scroll.unfilteredRanges = null; scroll.filtered = false;
  useRows.setState({ query: { ...defaultFilters, sort }, activeRow: rows[0] }); clearSelection();
  await reloadRows({ navigation: scroll, keepActive: true });
  if (useRows.getState().error) throw new Error(useRows.getState().error!);
}
