import assert from "node:assert/strict";
import test from "node:test";
import { galleryLayout, galleryCellPosition, GALLERY_PADDING, GALLERY_GAP } from "../src/lib/views/gallery/gallery-layout.ts";

test("gallery cards fit their viewport across slider sizes and narrow windows", () => {
  for (const width of [80, 255, 500, 900, 1600]) {
    for (const size of [120, 240, 400]) {
      const layout = galleryLayout(width, size, 48);
      assert.ok(layout.columns >= 1);
      assert.equal(layout.imageHeight, layout.cardWidth);
      const lastColumn = galleryCellPosition(layout.columns - 1, layout);
      assert.ok(lastColumn.x + layout.cardWidth <= width - GALLERY_PADDING);
      const nextRow = galleryCellPosition(layout.columns, layout);
      assert.equal(nextRow.x, GALLERY_PADDING);
      assert.ok(nextRow.y >= GALLERY_PADDING + layout.imageHeight + GALLERY_GAP);
      const lastCard = galleryCellPosition(47, layout);
      assert.ok(lastCard.y + layout.imageHeight < layout.spacerHeight);
    }
  }
});

test("empty and partial final rows have bounded space; restoring width restores positions", () => {
  assert.equal(galleryLayout(900, 200, 0).spacerHeight, 0);
  const original = galleryLayout(900, 200, 5);
  const smaller = galleryLayout(400, 200, 5);
  assert.ok(smaller.gridRows > original.gridRows);
  assert.deepEqual(galleryCellPosition(4, original), galleryCellPosition(4, galleryLayout(900, 200, 5)));
  const fullRows = galleryLayout(900, 200, original.columns * original.gridRows);
  assert.equal(original.spacerHeight, fullRows.spacerHeight);
});
