import { useLayoutEffect, useRef, useState, type UIEvent } from "react";
import { rememberVisibleRange, savedScrollPosition, saveScrollPosition } from "../../lib/stores/view-state";
import { useRows } from "../state/library";

/** A fixed virtual spacer is committed before position is restored in layout effect. */
export function useViewport(key: "gallery" | "table") {
  const viewport = useRef<HTMLDivElement>(null);
  const resetToken = useRows(state => state.resetToken);
  const [size, setSize] = useState(() => ({ width: 0, height: 0, top: savedScrollPosition(key), resetToken }));
  const loading = useRows(state => state.loading || state.refreshing);
  const restoring = useRef(false);
  useLayoutEffect(() => {
    const node = viewport.current;
    if (!node) return;
    const measure = () => {
      // Table columns resize in CSS; width changes do not affect its virtual rows.
      const width = key === "gallery" ? node.clientWidth : 0, height = node.clientHeight;
      setSize(previous => previous.width === width && previous.height === height ? previous : { ...previous, width, height });
    };
    const observer = new ResizeObserver(measure);
    observer.observe(node); measure();
    return () => observer.disconnect();
  }, [key]);
  useLayoutEffect(() => {
    const node = viewport.current;
    if (!node || loading || size.height <= 0) return;
    restoring.current = true;
    const top = savedScrollPosition(key);
    node.scrollTop = top;
    setSize(previous => ({ ...previous, top: node.scrollTop, resetToken }));
    restoring.current = false;
  }, [key, resetToken, loading, size.height]);
  const onScroll = (event: UIEvent<HTMLDivElement>) => {
    const top = event.currentTarget.scrollTop;
    setSize(previous => ({ ...previous, top }));
    if (!restoring.current && !loading) saveScrollPosition(key, top);
  };
  const rememberRange = (first: number, last: number) => {
    if (!loading) rememberVisibleRange(key, first, last);
  };
  // New pages and their target position must be rendered together. Rendering
  // with the old position first can unmount every card before layout restores it.
  const top = !loading && size.resetToken !== resetToken ? savedScrollPosition(key) : size.top;
  return { viewport, size: { ...size, top }, onScroll, rememberRange };
}
