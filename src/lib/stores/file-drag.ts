import { startDrag } from "@crabnebula/tauri-plugin-drag";
import { prepareFileDrag } from "../api";

const DRAG_THRESHOLD = 5;

export let outboundDrag = false;

let pending: {
  rowId: number;
  startX: number;
  startY: number;
  onDragStart?: () => void;
} | null = null;

function onMouseMove(e: MouseEvent): void {
  if (!pending) return;
  const dx = Math.abs(e.clientX - pending.startX);
  const dy = Math.abs(e.clientY - pending.startY);
  if (dx + dy < DRAG_THRESHOLD) return;

  const { rowId, onDragStart } = pending;
  cleanup();
  onDragStart?.();
  outboundDrag = true;
  prepareFileDrag(rowId).then(
    (info) =>
      startDrag({ item: [info.filePath], icon: info.iconPath }).finally(
        () => { outboundDrag = false; },
      ),
    () => { outboundDrag = false; },
  );
}

function onMouseUp(): void {
  cleanup();
}

function cleanup(): void {
  pending = null;
  window.removeEventListener("mousemove", onMouseMove);
  window.removeEventListener("mouseup", onMouseUp);
}

export function beginFileDrag(
  e: MouseEvent,
  rowId: number,
  onDragStart?: () => void,
): void {
  if (e.button !== 0) return;
  pending = { rowId, startX: e.clientX, startY: e.clientY, onDragStart };
  window.addEventListener("mousemove", onMouseMove);
  window.addEventListener("mouseup", onMouseUp, { once: true });
}
