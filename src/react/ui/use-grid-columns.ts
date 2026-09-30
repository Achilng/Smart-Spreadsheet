import { useLayoutEffect, useRef, useState } from "react";

/** Commit column changes through React so Motion can snapshot the previous rows. */
export function useGridColumns(minWidth: number, gap: number) {
  const ref = useRef<HTMLDivElement>(null);
  // Use the CSS auto-fill layout until measured, avoiding an initial one-column jump.
  const [columns, setColumns] = useState(0);
  useLayoutEffect(() => {
    const node = ref.current;
    if (!node) return;
    const update = (width: number) => setColumns(Math.max(1, Math.floor((width + gap) / (minWidth + gap))));
    const style = getComputedStyle(node);
    update(node.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight));
    const observer = new ResizeObserver(entries => update(entries[0].contentRect.width));
    observer.observe(node);
    return () => observer.disconnect();
  }, [minWidth, gap]);
  return { ref, columns };
}
