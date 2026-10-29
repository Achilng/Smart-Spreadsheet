import { type ReactNode } from "react";
import { useGridColumns } from "./use-grid-columns";
import { VirtualRows } from "./VirtualRows";

export function AlbumShelf({ label, count, itemKey, render }: { label: string; count: number; itemKey: (index: number) => string; render: (index: number) => ReactNode }) {
  const { ref, columns: measured } = useGridColumns(200, 16);
  const columns = measured || 1;
  return <div ref={ref} className="r-group-shelf r-virtual-shelf" role="list" aria-label={label} data-album-columns={columns}>
    <VirtualRows count={Math.ceil(count / columns)} layoutKey={columns} itemKey={index => itemKey(index * columns)} estimateSize={() => 306}
      render={() => null} renderCells={row => Array.from({ length: Math.min(columns, count - row * columns) }, (_, column) => {
          const index = row * columns + column;
          return { key: itemKey(index), className: "r-group-album-frame", content: render(index),
            style: { left: `calc(${column * 100 / columns}% + ${column * 16 / columns}px)`, width: `calc((100% - ${(columns - 1) * 16}px) / ${columns})`, height: 286 } };
        })} />
  </div>;
}
