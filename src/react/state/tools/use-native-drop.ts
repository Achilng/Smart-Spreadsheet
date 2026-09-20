import { useEffect, useRef, useState, type RefObject } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { errorText } from "../../../lib/utils/format";

export function useNativeDrop(active: boolean, disabled: boolean, target: RefObject<HTMLElement | null>, onDrop: (paths: string[]) => Promise<void>, onError: (message: string) => void) {
  const [dragging, setDragging] = useState(false);
  const latest = useRef({ active, disabled, onDrop, onError }); latest.current = { active, disabled, onDrop, onError };
  useEffect(() => {
    let disposed = false; let cleanup: (() => void) | undefined;
    const inside = (position: { x: number; y: number }) => {
      const rect = target.current?.getBoundingClientRect(); if (!rect) return false;
      const scale = window.devicePixelRatio || 1, x = position.x / scale, y = position.y / scale;
      return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
    };
    void getCurrentWebview().onDragDropEvent(event => {
      if (disposed) return;
      const state = latest.current;
      if (!state.active || state.disabled) { setDragging(false); return; }
      const payload = event.payload;
      if (payload.type === "enter" || payload.type === "over") setDragging(inside(payload.position));
      else { setDragging(false); if (payload.type === "drop" && inside(payload.position)) void state.onDrop(payload.paths).catch(error => state.onError(errorText(error))); }
    }).then(fn => { if (disposed) fn(); else cleanup = fn; }).catch(error => { if (!disposed) latest.current.onError(`拖放不可用：${errorText(error)}`); });
    return () => { disposed = true; cleanup?.(); };
  }, [target]);
  useEffect(() => { if (!active || disabled) setDragging(false); }, [active, disabled]);
  return dragging;
}
