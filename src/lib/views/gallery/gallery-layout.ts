export const GALLERY_GAP = 12;
export const GALLERY_PADDING = 16;
export const GALLERY_FOOTER_HEIGHT = 42;

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
