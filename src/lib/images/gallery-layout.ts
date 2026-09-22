export const GALLERY_GAP = 12;
export const GALLERY_PADDING = 16;
export const GALLERY_FOOTER_HEIGHT = 50; // 8px padding + 21px name + 1px gap + 20px metadata.

/** Shared square-card geometry for the gallery and material library. */
export function galleryLayout(viewportWidth: number, minCardWidth: number, count: number) {
  const columns = Math.max(1, Math.floor((viewportWidth - GALLERY_PADDING * 2 + GALLERY_GAP) / (minCardWidth + GALLERY_GAP)));
  const cardWidth = Math.max(1, Math.floor((viewportWidth - GALLERY_PADDING * 2 - GALLERY_GAP * (columns - 1)) / columns));
  const imageHeight = cardWidth;
  const cellHeight = imageHeight + GALLERY_FOOTER_HEIGHT + GALLERY_GAP;
  const gridRows = Math.ceil(count / columns);
  return {
    columns, cardWidth, imageHeight, cellHeight, gridRows,
    spacerHeight: gridRows === 0 ? 0 : GALLERY_PADDING * 2 + gridRows * cellHeight - GALLERY_GAP,
  };
}

export function galleryCellPosition(index: number, layout: ReturnType<typeof galleryLayout>) {
  return {
    x: GALLERY_PADDING + (index % layout.columns) * (layout.cardWidth + GALLERY_GAP),
    y: GALLERY_PADDING + Math.floor(index / layout.columns) * layout.cellHeight,
  };
}

/** The visible rows plus a small buffer, shared by both scrolling libraries. */
export function galleryVisibleIndices(layout: ReturnType<typeof galleryLayout>, scrollTop: number, viewportHeight: number, count: number, overscanRows = 2): number[] {
  if (viewportHeight <= 0 || count <= 0) return [];
  const firstRow = Math.max(0, Math.floor((scrollTop - GALLERY_PADDING) / layout.cellHeight) - overscanRows);
  const lastRow = Math.min(layout.gridRows, Math.ceil((scrollTop - GALLERY_PADDING + viewportHeight) / layout.cellHeight) + overscanRows);
  const first = Math.min(count, firstRow * layout.columns);
  const end = Math.min(count, lastRow * layout.columns);
  return Array.from({ length: Math.max(0, end - first) }, (_, index) => first + index);
}
