import { type ReactNode } from "react";
import { useAlbumReflow } from "./use-album-reflow";
import { useGridColumns } from "./use-grid-columns";
import { VirtualRows } from "./VirtualRows";

export function AlbumShelf({ label, count, itemKey, render }: { label: string; count: number; itemKey: (index: number) => string; render: (index: number) => ReactNode }) {
  const { ref, columns: measured } = useGridColumns(200, 16);
  const columns = measured || 1;
  useAlbumReflow(ref);
  return <div ref={ref} className="r-group-shelf r-virtual-shelf" role="list" aria-label={label} data-album-columns={columns}>
    <VirtualRows count={Math.ceil(count / columns)} layoutKey={columns} itemKey={index => itemKey(index * columns)} estimateSize={() => 306}
      render={row => <div className="r-virtual-shelf-row" style={{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}>
        {Array.from({ length: Math.min(columns, count - row * columns) }, (_, column) => {
          const index = row * columns + column;
          return <div className="r-group-album-frame" data-album-reflow key={itemKey(index)}>{render(index)}</div>;
        })}
      </div>} />
  </div>;
}
