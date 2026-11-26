import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { RowRecord } from "../../../lib/api";

const MIN_CARD = 180;
const COLUMN_GAP = 20;
const ROW_GAP = 28;
const CARD_EXTRA = 64;

export interface GridSection { key: string; heading?: ReactNode; rows: RowRecord[] }
type Line =
  | { kind: "heading"; key: string; content: ReactNode }
  | { kind: "cards"; key: string; rows: RowRecord[] }
  | { kind: "panel"; key: string }
  | { kind: "footer"; key: string };

/**
 * Windowed card grid for the results scroller: only the lines on screen are mounted.
 * `expanded` opens a full-width panel under the line holding that card.
 */
export function CompareGrid({ scrollRef, sections, renderCard, expanded, footer, onNearEnd, reveal }: {
  scrollRef: RefObject<HTMLElement | null>;
  sections: GridSection[];
  renderCard: (row: RowRecord) => ReactNode;
  expanded?: { rowId: number; content: ReactNode } | null;
  footer?: ReactNode;
  onNearEnd?: () => void;
  /** Scrolls to this card and focuses it whenever the object changes. */
  reveal?: { rowId: number } | null;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  const [margin, setMargin] = useState(0);
  useLayoutEffect(() => {
    const node = ref.current, scroller = scrollRef.current;
    if (!node || !scroller) return;
    const measure = () => {
      // While the side-by-side view covers the results they are display:none; keep the last layout.
      if (!node.clientWidth) return;
      setWidth(node.clientWidth);
      setMargin(node.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(node); observer.observe(scroller);
    if (node.previousElementSibling) observer.observe(node.previousElementSibling);
    return () => observer.disconnect();
  }, [scrollRef]);

  const columns = Math.max(1, Math.floor((width + COLUMN_GAP) / (MIN_CARD + COLUMN_GAP)));
  const cardWidth = width ? (width - COLUMN_GAP * (columns - 1)) / columns : MIN_CARD;
  const lines = useMemo(() => {
    const result: Line[] = [];
    for (const section of sections) {
      if (section.heading) result.push({ kind: "heading", key: `heading:${section.key}`, content: section.heading });
      for (let start = 0; start < section.rows.length; start += columns) {
        const rows = section.rows.slice(start, start + columns);
        result.push({ kind: "cards", key: `cards:${section.key}:${rows[0].id}`, rows });
        if (expanded && rows.some(row => row.id === expanded.rowId)) result.push({ kind: "panel", key: `panel:${expanded.rowId}` });
      }
    }
    if (footer) result.push({ kind: "footer", key: "footer" });
    return result;
  }, [sections, columns, expanded, footer]);

  const virtual = useVirtualizer({
    count: lines.length,
    getScrollElement: () => scrollRef.current,
    getItemKey: index => lines[index].key,
    estimateSize: index => {
      const line = lines[index];
      return line.kind === "cards" ? cardWidth * 1.25 + CARD_EXTRA + ROW_GAP : line.kind === "panel" ? 320 : line.kind === "heading" ? 56 : 72;
    },
    overscan: 3,
    scrollMargin: margin,
  });
  useLayoutEffect(() => { virtual.measure(); }, [cardWidth, virtual]);

  const items = virtual.getVirtualItems();
  const lastIndex = items.at(-1)?.index ?? -1;
  useEffect(() => { if (lastIndex >= lines.length - 4) onNearEnd?.(); }, [lastIndex, lines.length, onNearEnd]);

  useEffect(() => {
    if (!reveal) return;
    const index = lines.findIndex(line => line.kind === "cards" && line.rows.some(row => row.id === reveal.rowId));
    if (index < 0) return;
    virtual.scrollToIndex(index, { align: "auto" });
    // The card mounts once the virtualizer has handled the scroll, a frame or two later.
    let frame = 0, attempts = 0;
    const focus = () => {
      const card = ref.current?.querySelector<HTMLElement>(`[data-compare-id="${reveal.rowId}"]`);
      if (card) card.focus({ preventScroll: true });
      else if (++attempts < 20) frame = requestAnimationFrame(focus);
    };
    frame = requestAnimationFrame(focus);
    return () => cancelAnimationFrame(frame);
  }, [reveal]);

  return <div ref={ref} className="rc-grid" style={{ height: virtual.getTotalSize(), position: "relative" }}>
    {items.map(item => {
      const line = lines[item.index];
      return <div key={item.key} data-index={item.index} ref={virtual.measureElement} className={`rc-grid-line is-${line.kind}`}
        style={{ position: "absolute", top: 0, left: 0, width: "100%", transform: `translateY(${item.start - margin}px)` }}>
        {line.kind === "cards" ? <div className="rc-grid-cards" style={{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}>{line.rows.map(row => <div key={row.id} className="rc-grid-cell">{renderCard(row)}</div>)}</div>
          : line.kind === "heading" ? line.content
            : line.kind === "panel" ? expanded?.content
              : footer}
      </div>;
    })}
  </div>;
}
