import { useEffect, useState } from "react";
import { ImageLoader, isImageLoadCancelled } from "../../lib/images/image-loader";
import { getRowThumbnail } from "../../lib/api";

export const thumbnails = new ImageLoader(getRowThumbnail, 6, 360, "image/png");

export function useImage(loader: ImageLoader, rowId: number | undefined) {
  const [image, setImage] = useState<{ id: number | undefined; url: string | null; error: boolean }>({ id: rowId, url: rowId === undefined ? null : loader.cached(rowId), error: false });
  useEffect(() => {
    let disposed = false;
    if (rowId === undefined) return;
    const cached = loader.cached(rowId);
    setImage({ id: rowId, url: cached, error: false });
    if (!cached) void loader.load(rowId).then(url => {
      if (!disposed) setImage({ id: rowId, url, error: false });
    }, error => {
      if (!disposed && !isImageLoadCancelled(error)) setImage({ id: rowId, url: null, error: true });
    });
    return () => { disposed = true; };
  }, [loader, rowId]);
  return image.id === rowId ? image : { id: rowId, url: null, error: false };
}
