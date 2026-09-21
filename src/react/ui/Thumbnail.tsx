import { memo, useEffect, useRef, useState } from "react";
import { ImageOff } from "lucide-react";
import { useProgressiveImage } from "./use-image";
import { galleryPreviews } from "../../lib/images/progressive-images";

export const Thumbnail = memo(function Thumbnail({ rowId, detail = false, enhanced = false, hasImage = true, alt, className }: { rowId?: number; detail?: boolean; enhanced?: boolean; hasImage?: boolean; alt: string; className?: string }) {
  const root = useRef<HTMLDivElement>(null);
  const [readyId, setReadyId] = useState<number | undefined>(() => enhanced && rowId !== undefined && galleryPreviews.cached(rowId) ? rowId : undefined);
  useEffect(() => {
    if (!enhanced || rowId === undefined || !root.current) return;
    const scrollRoot = root.current.closest(".r-gallery");
    let visible = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const settled = () => {
      clearTimeout(timer);
      if (!visible) return;
      if (galleryPreviews.cached(rowId)) setReadyId(rowId);
      else timer = setTimeout(() => setReadyId(rowId), 200);
    };
    const observer = new IntersectionObserver(entries => {
      visible = entries.some(entry => entry.isIntersecting);
      // Leaving the viewport only cancels a pending upgrade. Keep an already
      // loaded preview while this virtual card remains mounted.
      if (visible) settled(); else clearTimeout(timer);
    }, { root: scrollRoot });
    observer.observe(root.current);
    scrollRoot?.addEventListener("scroll", settled, { passive: true });
    return () => { observer.disconnect(); clearTimeout(timer); scrollRoot?.removeEventListener("scroll", settled); };
  }, [enhanced, rowId]);
  const image = useProgressiveImage(hasImage ? rowId : undefined, detail ? "detail" : enhanced && readyId === rowId ? "gallery" : "thumbnail");
  return <div ref={root} className={`r-image ${className ?? ""}`} data-loaded={Boolean(image.url)} onDoubleClick={event => { if (image.error) { event.stopPropagation(); image.retry(); } }}>
    {!hasImage ? <span className="r-image-error"><ImageOff size={20} /><span>无图片</span></span> : <>
      {image.thumbnailUrl ? <img className="r-image-base" src={image.thumbnailUrl} alt={alt} decoding="async" draggable={false} /> : image.error && !image.previewUrl ? <span className="r-image-error" title="双击重试"><ImageOff size={20} /><span>图片无法读取</span></span> : <span className="r-image-placeholder" aria-label="正在加载图片" />}
      {image.previewUrl && <DecodedPreview key={image.previewUrl} url={image.previewUrl} alt={image.thumbnailUrl ? "" : alt} />}
    </>}
  </div>;
});

/** Never replace the displayed thumbnail with bytes the browser has not decoded. */
function DecodedPreview({ url, alt }: { url: string; alt: string }) {
  const element = useRef<HTMLImageElement>(null);
  const [ready, setReady] = useState(false);
  useEffect(() => {
    const image = element.current;
    if (!image) return;
    let disposed = false;
    let frame = 0;
    void image.decode().then(() => {
      if (disposed) return;
      frame = requestAnimationFrame(() => { if (!disposed) setReady(true); });
    }).catch(() => { /* Keep the thumbnail if the preview cannot be decoded. */ });
    return () => { disposed = true; cancelAnimationFrame(frame); };
  }, [url]);
  return <img ref={element} className="r-image-preview" data-ready={ready} src={url} alt={alt} aria-hidden={!alt || undefined} decoding="async" draggable={false} />;
}
