import { useLayoutEffect, useRef, useState, type UIEvent } from "react";
import { rememberVisibleRange, savedScrollPosition, saveScrollPosition } from "../../lib/stores/view-state";
import { useRows } from "../state/library";
import { galleryCellPosition, galleryLayout, GALLERY_PADDING_TOP } from "../../lib/images/gallery-layout";

interface GalleryAnchor {
  index: number;
  offset: number;
  top: number;
  resetToken: number;
}

/** A fixed virtual spacer is committed before position is restored in layout effect. */
export function useViewport(key: "gallery" | "table", gallery?: { cardSize: number; total: number }) {
  const viewport = useRef<HTMLDivElement>(null);
  const resetToken = useRows(state => state.resetToken);
  const [size, setSize] = useState(() => ({ width: 0, height: 0, top: savedScrollPosition(key), resetToken }));
  const loading = useRows(state => state.loading || state.refreshing);
  const restoring = useRef(false);
  const anchor = useRef<GalleryAnchor | null>(null);
  const ready = size.height > 0;
  const layout = gallery && size.width > 0 ? galleryLayout(size.width, gallery.cardSize, gallery.total) : null;
  const previous = anchor.current;
  // Project the same picture into the new grid before rendering virtual cells.
  // Keep its identity across every frame, even when it is no longer the first
  // column: recapturing the first column on each resize causes cumulative drift.
  // A new result set takes precedence: render its saved position and pages
  // together, then capture a fresh anchor after the spacer has been committed.
  const top = !loading && size.resetToken !== resetToken ? savedScrollPosition(key)
    : layout && gallery && gallery.total > 0 && previous?.resetToken === resetToken
      ? Math.max(0, Math.min(layout.spacerHeight - size.height,
        galleryCellPosition(Math.min(previous.index, gallery.total - 1), layout).y + Math.min(previous.offset, layout.cellHeight - 1)))
      : size.top;
  const captureAnchor = (position: number) => {
    if (!layout || !gallery?.total || loading) { anchor.current = null; return; }
    const index = Math.min(gallery.total - 1, Math.max(0, Math.floor((position - GALLERY_PADDING_TOP) / layout.cellHeight)) * layout.columns);
    anchor.current = { index, offset: position - galleryCellPosition(index, layout).y, top: position, resetToken };
  };
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
    if (!node || loading || !ready) return;
    restoring.current = true;
    const top = savedScrollPosition(key);
    node.scrollTop = top;
    setSize(previous => previous.top === node.scrollTop && previous.resetToken === resetToken ? previous : { ...previous, top: node.scrollTop, resetToken });
    restoring.current = false;
  }, [key, resetToken, loading, ready]);
  useLayoutEffect(() => {
    const node = viewport.current;
    if (!node || !layout || !ready || loading) return;
    node.scrollTop = top;
    const applied = node.scrollTop;
    if (!anchor.current || anchor.current.resetToken !== resetToken) captureAnchor(applied);
    else anchor.current.top = applied;
    setSize(previous => previous.top === applied ? previous : { ...previous, top: applied });
    saveScrollPosition(key, applied);
  }, [key, top, layout?.columns, layout?.cellHeight, layout?.spacerHeight, ready, size.height, resetToken, loading]);
  const onScroll = (event: UIEvent<HTMLDivElement>) => {
    const top = event.currentTarget.scrollTop;
    // Ignore our own scroll events; only a new browsing position changes the
    // anchor. User scrolling during a resize immediately takes precedence.
    if (!restoring.current && !loading && (!anchor.current || Math.abs(anchor.current.top - top) > 1)) captureAnchor(top);
    setSize(previous => ({ ...previous, top }));
    if (!restoring.current && !loading) saveScrollPosition(key, top);
  };
  const rememberRange = (first: number, last: number) => {
    if (!loading) rememberVisibleRange(key, first, last);
  };
  return { viewport, size: { ...size, top }, onScroll, rememberRange };
}
