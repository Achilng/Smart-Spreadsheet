import { useLayoutEffect, useRef, useState, type UIEvent } from "react";
import { rememberVisibleRange, savedScrollPosition, saveScrollPosition } from "../../lib/stores/view-state";
import { useRows } from "../state/library";

/** A fixed virtual spacer is committed before position is restored in layout effect. */
export function useViewport(key: "gallery" | "table") {
  const viewport = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState(() => ({ width: 0, height: 0, top: savedScrollPosition(key) }));
  const resetToken = useRows(state => state.resetToken);
  const loading = useRows(state => state.loading || state.refreshing);
  const restoring = useRef(false);
  useLayoutEffect(() => {
    const node = viewport.current;
    if (!node) return;
    const measure = () => setSize(previous => ({ ...previous, width: node.clientWidth, height: node.clientHeight }));
    const observer = new ResizeObserver(measure);
    observer.observe(node); measure();
    return () => observer.disconnect();
  }, []);
  useLayoutEffect(() => {
    const node = viewport.current;
    if (!node || loading || size.height <= 0) return;
    restoring.current = true;
    const top = savedScrollPosition(key);
    node.scrollTop = top;
    setSize(previous => ({ ...previous, top: node.scrollTop }));
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
  return { viewport, size, onScroll, rememberRange };
}
