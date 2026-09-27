export interface ImageSize { width: number; height: number }
export interface ImagePoint { x: number; y: number }
export const LIGHTBOX_PADDING = 24;
export const MAX_IMAGE_SCALE = 8;

export function imageFitScale(image: ImageSize, viewport: ImageSize): number {
  if (image.width <= 0 || image.height <= 0 || viewport.width <= 0 || viewport.height <= 0) return 0;
  return Math.min(1, Math.max(1, viewport.width - LIGHTBOX_PADDING * 2) / image.width, Math.max(1, viewport.height - LIGHTBOX_PADDING * 2) / image.height);
}
export function clampImagePan(pan: ImagePoint, image: ImageSize, viewport: ImageSize, scale: number): ImagePoint {
  const limitX = Math.max(0, (image.width * scale - Math.max(1, viewport.width - LIGHTBOX_PADDING * 2)) / 2);
  const limitY = Math.max(0, (image.height * scale - Math.max(1, viewport.height - LIGHTBOX_PADDING * 2)) / 2);
  return { x: limitX ? Math.max(-limitX, Math.min(limitX, pan.x)) : 0, y: limitY ? Math.max(-limitY, Math.min(limitY, pan.y)) : 0 };
}
export function zoomImageAt(pan: ImagePoint, point: ImagePoint, previousScale: number, nextScale: number): ImagePoint {
  if (previousScale <= 0) return { x: 0, y: 0 };
  const ratio = nextScale / previousScale;
  return { x: point.x - (point.x - pan.x) * ratio, y: point.y - (point.y - pan.y) * ratio };
}
