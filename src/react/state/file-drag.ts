import { startDrag } from "@crabnebula/tauri-plugin-drag";
import { prepareFileDrag } from "../../lib/api";
import { errorText } from "../../lib/utils/format";
import { notify } from "./notices";
import { isSelected, selectionDto } from "./selection";

const DRAG_THRESHOLD = 5;
export let outboundDrag = false;
let pending: { rowId: number; startX: number; startY: number; onDragStart?: () => void; onDragEnd?: () => void } | null = null;

export function cancelPendingFileDrag(): void {
  pending = null;
  window.removeEventListener("mousemove", onMouseMove);
  window.removeEventListener("mouseup", cancelPendingFileDrag);
  window.removeEventListener("blur", cancelPendingFileDrag);
}
function onMouseMove(event: MouseEvent): void {
  if (!pending) return;
  if (!(event.buttons & 1)) { cancelPendingFileDrag(); return; }
  if (Math.abs(event.clientX - pending.startX) + Math.abs(event.clientY - pending.startY) < DRAG_THRESHOLD) return;
  const { rowId, onDragStart, onDragEnd } = pending;
  cancelPendingFileDrag();
  onDragStart?.();
  outboundDrag = true;
  const selection = isSelected(rowId) ? selectionDto() : null;
  void (async () => {
    try {
      const info = await prepareFileDrag(rowId, selection);
      await startDrag({ item: info.filePaths, icon: info.iconPath });
    } catch (error) { notify(`拖出图片失败：${errorText(error)}`, "error"); }
    finally { outboundDrag = false; onDragEnd?.(); }
  })();
}
export function beginFileDrag(event: MouseEvent, rowId: number, onDragStart?: () => void, onDragEnd?: () => void): void {
  if (event.button !== 0 || outboundDrag) return;
  cancelPendingFileDrag();
  pending = { rowId, startX: event.clientX, startY: event.clientY, onDragStart, onDragEnd };
  window.addEventListener("mousemove", onMouseMove);
  window.addEventListener("mouseup", cancelPendingFileDrag, { once: true });
  window.addEventListener("blur", cancelPendingFileDrag, { once: true });
}
if (import.meta.hot) import.meta.hot.dispose(cancelPendingFileDrag);
