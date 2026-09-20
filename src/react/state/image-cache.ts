import { create } from "zustand";
import { getRowThumbnail } from "../../lib/api";
import { ImageLoader } from "../../lib/images/image-loader";
import { detailPreviews, galleryPreviews, originalImages } from "../../lib/images/progressive-images";
import { vibeStatuses } from "../../lib/images/vibe-statuses";

export const thumbnails = new ImageLoader(getRowThumbnail, 6, 360, "image/png");
export const useImageCache = create<{ version: number }>(() => ({ version: 0 }));
export function invalidateImageCache(): void {
  thumbnails.clear(); galleryPreviews.clear(); detailPreviews.clear(); originalImages.clear(); vibeStatuses.clear();
  useImageCache.setState(state => ({ version: state.version + 1 }));
}
