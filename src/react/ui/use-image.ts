import { useCallback, useEffect, useState } from "react";
import { ImageLoader, isImageLoadCancelled } from "../../lib/images/image-loader";
import { useImageCache } from "../state/image-cache";
import { thumbnails } from "../state/image-cache";
import { detailPreviews, galleryPreviews } from "../../lib/images/progressive-images";
export { thumbnails } from "../state/image-cache";

export function useImage(loader: ImageLoader, rowId: number | undefined, priority = false) {
  const version = useImageCache(state => state.version);
  const [revision, setRevision] = useState(0);
  const retry = useCallback(() => setRevision(value => value + 1), []);
  const [image, setImage] = useState(() => ({ loader, id: rowId, version, url: rowId === undefined ? null : loader.cached(rowId), error: false }));
  useEffect(() => {
    let disposed = false;
    if (rowId === undefined) return;
    const cached = loader.cached(rowId);
    setImage({ loader, id: rowId, version, url: cached, error: false });
    if (!cached) void loader.load(rowId, priority).then(url => {
      if (!disposed) setImage({ loader, id: rowId, version, url, error: false });
    }, error => {
      if (!disposed && !isImageLoadCancelled(error)) setImage({ loader, id: rowId, version, url: null, error: true });
    });
    return () => { disposed = true; };
  }, [loader, rowId, version, revision, priority]);
  // Cached tiers are available in the first render, without a placeholder frame.
  return { ...(image.loader === loader && image.id === rowId && image.version === version ? image : { id: rowId, url: rowId === undefined ? null : loader.cached(rowId), error: false }), retry };
}

/** Keep a lower-resolution image visible until the higher-resolution layer arrives. */
export function useProgressiveImage(rowId: number | undefined, tier: "thumbnail" | "gallery" | "detail" = "detail") {
  const thumb = useImage(thumbnails, rowId);
  const preview = useImage(tier === "detail" ? detailPreviews : galleryPreviews, tier === "thumbnail" ? undefined : rowId, true);
  const cachedGallery = tier === "detail" && rowId !== undefined ? galleryPreviews.cached(rowId) : null;
  const previewUrl = preview.url ?? cachedGallery;
  const retry = useCallback(() => { thumb.retry(); preview.retry(); }, [thumb.retry, preview.retry]);
  return { url: previewUrl ?? thumb.url, thumbnailUrl: thumb.url, previewUrl, error: tier === "thumbnail" ? thumb.error : preview.error, retry };
}
