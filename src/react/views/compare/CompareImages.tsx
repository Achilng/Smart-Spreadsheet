import { useEffect, useRef, useState } from "react";
import { ImageOff, RefreshCw } from "lucide-react";
import type { RowRecord } from "../../../lib/api";
import { thumbnails, useProgressiveImage } from "../../ui/use-image";
import { detailPreviews, galleryPreviews } from "../../../lib/images/progressive-images";
import { rowFileName } from "../../../lib/utils/row-display";
export { ImageLightbox as OriginalLightbox } from "../../ui/ImageLightbox";

export function PaneImage({ row, tier = "detail", lazy = false }: { row: RowRecord; tier?: "thumbnail" | "gallery" | "detail"; lazy?: boolean }) {
  const ref = useRef<HTMLSpanElement>(null);
  const identity = `${row.id}:${tier}`;
  const [visible, setVisible] = useState<string | null>(null);
  const [broken, setBroken] = useState<string[]>([]);
  const hasImage = Boolean(row.imagePath?.trim() || row.storedImagePath?.trim());
  const enabled = !lazy || visible === identity;
  const image = useProgressiveImage(hasImage ? row.id : undefined, tier, enabled);
  useEffect(() => {
    if (!lazy || !hasImage || !ref.current || visible === identity) return;
    const observer = new IntersectionObserver(entries => {
      if (entries.some(entry => entry.isIntersecting)) { setVisible(identity); observer.disconnect(); }
    }, { rootMargin: "160px" });
    observer.observe(ref.current);
    return () => observer.disconnect();
  }, [identity, visible, hasImage, lazy]);
  const url = [image.url, image.thumbnailUrl].find(value => value && !broken.includes(value));
  const failed = !url && (image.error || Boolean(image.url && broken.includes(image.url)));
  function retry() {
    for (const loader of [thumbnails, galleryPreviews, detailPreviews]) {
      const cached = loader.cached(row.id);
      if (cached && broken.includes(cached)) loader.invalidate(row.id, cached);
    }
    setBroken([]);
    image.retry();
  }
  return <span className="rc-image" ref={ref}>
    {url ? <img src={url} alt={rowFileName(row) || `图片 ${row.id}`} decoding="async" draggable={false} onError={() => setBroken(current => [...current.slice(-3), url])} />
      : !hasImage || failed ? <span className="rc-image-unavailable"><ImageOff size={23} /><small>{failed ? "预览不可用" : "无图片"}</small>{failed && <button type="button" className="image-retry" aria-label={`重新加载 ${rowFileName(row) || `图片 ${row.id}`}`} onClick={event => { event.stopPropagation(); retry(); }}><RefreshCw size={12} />重新加载</button>}</span>
      : <span className="rc-image-loading" aria-label="正在加载图片" />}
  </span>;
}
