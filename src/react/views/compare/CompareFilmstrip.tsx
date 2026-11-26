import { useEffect, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { RowRecord } from "../../../lib/api";
import { useReducedMotionPreference } from "../../ui/use-reduced-motion";
import { PaneImage } from "./CompareImages";
import { rowDetails } from "./CompareResults";

const ITEM_WIDTH = 54;
const ITEM_GAP = 6;

/** Every result B can move to, in order; the current B stays centred. */
export function CompareFilmstrip({ rows, currentId, more, onPick, onNearEnd }: {
  rows: RowRecord[]; currentId: number; more: boolean; onPick: (row: RowRecord) => void; onNearEnd: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const reducedMotion = useReducedMotionPreference();
  const count = rows.length + (more ? 1 : 0);
  const virtual = useVirtualizer({ horizontal: true, count, getScrollElement: () => ref.current, estimateSize: () => ITEM_WIDTH + ITEM_GAP, overscan: 8, paddingStart: 2, paddingEnd: 2 });
  const currentIndex = rows.findIndex(row => row.id === currentId);
  const placed = useRef(false);
  useEffect(() => {
    if (currentIndex < 0) return;
    virtual.scrollToIndex(currentIndex, { align: "center", behavior: placed.current && !reducedMotion ? "smooth" : "auto" });
    placed.current = true;
  }, [currentIndex, virtual, reducedMotion]);
  const items = virtual.getVirtualItems();
  const lastIndex = items.at(-1)?.index ?? -1;
  useEffect(() => { if (more && lastIndex >= count - 4) onNearEnd(); }, [more, lastIndex, count, onNearEnd]);
  return <div ref={ref} className="filmstrip" aria-label="切换对比目标 B"
    onWheel={event => { if (ref.current && Math.abs(event.deltaY) > Math.abs(event.deltaX)) ref.current.scrollLeft += event.deltaY; }}>
    <div style={{ width: virtual.getTotalSize(), height: "100%", position: "relative" }}>
      {items.map(item => {
        const row = rows[item.index];
        const style = { left: item.start, width: ITEM_WIDTH };
        if (!row) return <span key="more" className="film-item is-more" style={style}><span className="rc-image-loading" /></span>;
        const current = row.id === currentId;
        return <button key={row.id} type="button" className={`film-item${current ? " is-current" : ""}`} style={style} aria-current={current || undefined}
          title={rowDetails(row)} aria-label={`第 ${item.index + 1} 张`} onClick={() => onPick(row)}>
          <PaneImage row={row} tier="thumbnail" />
        </button>;
      })}
    </div>
  </div>;
}
