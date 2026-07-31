import { startDrag } from "@crabnebula/tauri-plugin-drag";
import { prepareFileDrag } from "../api";
import { errorText, setNotice } from "./app-state.svelte";
import { isRowSelected, selectionDto } from "./selection-store.svelte";

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
  // 资源管理器惯例：从选中项开始拖动时带出整个选区；未选中项仍只拖自身。
  // 后端会逐项解析完整原件，任何项目都不会用缩略图或预览图替代。
  const dragSelection = isRowSelected(rowId) ? selectionDto() : null;
  prepareFileDrag(rowId, dragSelection).then(
    (info) =>
      startDrag({ item: info.filePaths, icon: info.iconPath }).finally(
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
