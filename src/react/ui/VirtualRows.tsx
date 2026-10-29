import { useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { defaultRangeExtractor, useVirtualizer } from "@tanstack/react-virtual";
import { GalleryLayoutItem } from "./GalleryMotion";
import { useReducedMotionPreference } from "./use-reduced-motion";

const noStickyRows: number[] = [];

/** Shared windowing for shelves, section headers and image rows in SectionList. */
export function VirtualRows({ count, itemKey, estimateSize, render, renderCells, layoutKey, stickyRows = noStickyRows }: {
  count: number; itemKey: (index: number) => string; estimateSize: (index: number) => number;
  render: (index: number) => ReactNode; layoutKey: number; stickyRows?: number[];
  renderCells?: (index: number) => { key: string; content: ReactNode; style: CSSProperties; className?: string }[] | undefined;
}) {
  const reducedMotion = useReducedMotionPreference();
  const ref = useRef<HTMLDivElement>(null);
  const activeSticky = useRef(-1);
  const [margin, setMargin] = useState(0);
  const virtual = useVirtualizer({
    count, getItemKey: itemKey, estimateSize, overscan: 3, scrollMargin: margin,
    getScrollElement: () => ref.current?.closest<HTMLElement>(".r-section-list") ?? null,
    rangeExtractor: range => {
      // Keep the current section heading mounted without retaining previous sections.
      let low = 0, high = stickyRows.length;
      while (low < high) {
        const middle = (low + high) >>> 1;
        if (stickyRows[middle] <= range.startIndex) low = middle + 1; else high = middle;
      }
      activeSticky.current = stickyRows[low - 1] ?? -1;
      const visible = defaultRangeExtractor(range);
      return activeSticky.current < 0 ? visible : [...new Set([activeSticky.current, ...visible])].sort((a, b) => a - b);
    },
  });
  useLayoutEffect(() => {
    const node = ref.current, viewport = node?.closest<HTMLElement>(".r-section-list");
    if (!node || !viewport) return;
    const measure = () => setMargin(node.getBoundingClientRect().top - viewport.getBoundingClientRect().top + viewport.scrollTop);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(viewport); observer.observe(node);
    return () => observer.disconnect();
  }, []);
  useLayoutEffect(() => { virtual.measure(); }, [layoutKey, virtual]);
  return <div ref={ref} className="r-virtual-rows" style={{ height: virtual.getTotalSize(), position: "relative" }}>
    {virtual.getVirtualItems().flatMap(item => {
      const cells = renderCells?.(item.index);
      // Keep cards as siblings, like Gallery: changing columns or filters must not
      // move an image to a new React parent and discard its previous layout.
      if (cells) return cells.map(cell => <GalleryLayoutItem key={`cell:${cell.key}`}
        layoutDependency={`${layoutKey}:${item.start}:${cell.style.left}:${reducedMotion}`}
        className={cell.className} style={{ position: "absolute", top: item.start - margin, height: item.size, ...cell.style }}>
        {cell.content}
      </GalleryLayoutItem>);
      return [<div key={item.key} data-index={item.index} ref={virtual.measureElement}
      style={{ position: item.index === activeSticky.current ? "sticky" : "absolute", zIndex: item.index === activeSticky.current ? 2 : undefined,
        top: 0, left: 0, width: "100%", transform: item.index === activeSticky.current ? undefined : `translateY(${item.start - margin}px)` }}>
      {render(item.index)}
    </div>]; })}
  </div>;
}
