import { useEffect, useRef, useState } from "react";
import { ImageOff } from "lucide-react";
import type { RowRecord } from "../../../lib/api";
import { useProgressiveImage } from "../../ui/use-image";
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
  return <span className="rc-image" ref={ref} onDoubleClick={event => {
    if (failed) { event.stopPropagation(); setBroken([]); image.retry(); }
  }}>
    {url ? <img src={url} alt={rowFileName(row) || `图片 ${row.id}`} decoding="async" draggable={false} onError={() => setBroken(current => [...current.slice(-3), url])} />
      : !hasImage || failed ? <span className="rc-image-unavailable"><ImageOff size={23} /><small>{failed ? "预览不可用" : "无图片"}</small></span>
      : <span className="rc-image-loading" aria-label="正在加载图片" />}
  </span>;
}
