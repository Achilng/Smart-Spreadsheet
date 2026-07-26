import { startDrag } from "@crabnebula/tauri-plugin-drag";
import { prepareFileDrag } from "../api";
import { errorText, setNotice } from "./app-state.svelte";

const DRAG_THRESHOLD = 5;

export let outboundDrag = false;

let pending: {
  rowId: number;
  startX: number;
  startY: number;
  onDragStart?: () => void;
  onDragEnd?: () => void;
} | null = null;

function onMouseMove(e: MouseEvent): void {
  if (!pending) return;
  const dx = Math.abs(e.clientX - pending.startX);
  const dy = Math.abs(e.clientY - pending.startY);
  if (dx + dy < DRAG_THRESHOLD) return;

  const { rowId, onDragStart, onDragEnd } = pending;
  cleanup();
  onDragStart?.();
  outboundDrag = true;
  prepareFileDrag(rowId).then(
    (info) =>
      startDrag({ item: [info.filePath], icon: info.iconPath }).finally(
        () => {
          outboundDrag = false;
          // OS 级拖拽结束后浏览器不会派发 click，必须主动通知调用方复位状态，
          // 否则下一次点击会被“拖拽后吞 click”的守卫吃掉（死点击）。
          onDragEnd?.();
        },
      ),
    (error) => {
      outboundDrag = false;
      onDragEnd?.();
      setNotice({ tone: "error", text: errorText(error) });
    },
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
  onDragEnd?: () => void,
): void {
  if (e.button !== 0) return;
  pending = { rowId, startX: e.clientX, startY: e.clientY, onDragStart, onDragEnd };
  window.addEventListener("mousemove", onMouseMove);
  window.addEventListener("mouseup", onMouseUp, { once: true });
}
