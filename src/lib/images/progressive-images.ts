import {
  getRowGalleryPreview,
  getRowOriginal,
  getRowPreview,
} from "../api";
import { ImageLoader } from "./image-loader";

/** 画廊停滚后加载的 1024px 高清层：低并发，避免与缩略图争抢解码资源。 */
export const galleryPreviews = new ImageLoader(getRowGalleryPreview, 2, 48, "image/png");

/** 详情面板使用的 2048px 预览层。 */
export const detailPreviews = new ImageLoader(getRowPreview, 2, 8, "image/png");

/** 全屏查看时才读取的完整 PNG 原图。 */
export const originalImages = new ImageLoader(getRowOriginal, 1, 1, "image/png");

export function clearProgressiveImages(): void {
  galleryPreviews.clear();
  detailPreviews.clear();
  originalImages.clear();
}
