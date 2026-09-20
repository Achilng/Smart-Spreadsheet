import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Dialog } from "radix-ui";
import { ImageOff, X } from "lucide-react";
import type { RowRecord } from "../../../lib/api";
import { galleryPreviews, detailPreviews, originalImages } from "../../../lib/images/progressive-images";
import { isImageLoadCancelled } from "../../../lib/images/image-loader";
import { thumbnails } from "../../ui/use-image";
import { rowFileName } from "../../../lib/utils/row-display";
import { Button } from "../../ui/controls";

export function PaneImage({ row, tier = "detail", lazy = false }: { row: RowRecord; tier?: "thumbnail" | "gallery" | "detail"; lazy?: boolean }) {
  const ref = useRef<HTMLSpanElement>(null);
  const [image, setImage] = useState<{ id: number; url: string | null; failed: boolean }>({ id: row.id, url: null, failed: false });
  const hasImage = Boolean(row.imagePath?.trim() || row.storedImagePath?.trim());
  useEffect(() => {
    let disposed = false;
    let observer: IntersectionObserver | undefined;
    const loader = tier === "gallery" ? galleryPreviews : tier === "detail" ? detailPreviews : thumbnails;
    const cached = loader.cached(row.id) ?? thumbnails.cached(row.id);
    let currentUrl = cached;
    setImage({ id: row.id, url: cached, failed: false });
    if (!hasImage) return;
    const start = () => {
      observer?.disconnect();
      if (tier === "thumbnail") {
        void thumbnails.load(row.id).then(url => { if (!disposed) setImage({ id: row.id, url, failed: false }); }, error => { if (!disposed && !isImageLoadCancelled(error)) setImage({ id: row.id, url: null, failed: true }); });
        return;
      }
      let failed = 0;
      const failure = (error: unknown) => {
        if (!isImageLoadCancelled(error)) failed++;
        if (!disposed && failed === 2 && !currentUrl) setImage({ id: row.id, url: null, failed: true });
      };
      void thumbnails.load(row.id).then(url => {
        if (!disposed && !currentUrl) { currentUrl = url; setImage({ id: row.id, url, failed: false }); }
      }, failure);
      void loader.load(row.id, true).then(url => {
        if (!disposed) { currentUrl = url; setImage({ id: row.id, url, failed: false }); }
      }, failure);
    };
    if (lazy && ref.current && !cached) {
      observer = new IntersectionObserver(entries => { if (entries.some(entry => entry.isIntersecting)) start(); }, { rootMargin: "160px" });
      observer.observe(ref.current);
    } else start();
    return () => { disposed = true; observer?.disconnect(); };
  }, [row.id, hasImage, tier, lazy]);
  const current = image.id === row.id ? image : { url: null, failed: false };
  return <span className="rc-image" ref={ref}>
    {current.url ? <img src={current.url} alt={rowFileName(row) || `图片 ${row.id}`} decoding="async" draggable={false} onError={() => setImage({ id: row.id, url: null, failed: true })} />
      : !hasImage || current.failed ? <span className="rc-image-unavailable"><ImageOff size={23} /><small>{current.failed ? "预览不可用" : "无图片"}</small></span>
      : <span className="rc-image-loading" aria-label="正在加载图片" />}
  </span>;
}

export function OriginalLightbox({ row, onClose }: { row: RowRecord; onClose: () => void }) {
  const [url, setUrl] = useState<string | null>(() => originalImages.cached(row.id));
  const [preview, setPreview] = useState<string | null>(() => detailPreviews.cached(row.id) ?? galleryPreviews.cached(row.id) ?? thumbnails.cached(row.id));
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const opener = useRef<HTMLElement | null>(null);
  useLayoutEffect(() => { opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null; }, []);
  useEffect(() => {
    let disposed = false;
    setError(null);
    originalImages.retain(new Set([row.id]));
    void originalImages.load(row.id, true).then(loaded => { if (!disposed) setUrl(loaded); }, reason => { if (!disposed && !isImageLoadCancelled(reason)) setError(String(reason)); });
    if (!preview) void detailPreviews.load(row.id, true).then(loaded => { if (!disposed) setPreview(loaded); }, () => {});
    return () => { disposed = true; originalImages.retain(new Set()); };
  }, [row.id, revision]);
  const title = rowFileName(row) || `第 ${row.sourceOrdinal} 张`;
  return <Dialog.Root open onOpenChange={open => { if (!open) onClose(); }}><Dialog.Portal>
    <Dialog.Overlay className="rc-lightbox-overlay" />
    <Dialog.Content className="rc-lightbox" aria-describedby={undefined} onCloseAutoFocus={event => { event.preventDefault(); opener.current?.focus(); }} onEscapeKeyDown={event => event.stopPropagation()}>
      <header><Dialog.Title className="rc-lightbox-title" title={title}>{title}</Dialog.Title><span className={error ? "rc-lightbox-error" : ""} role="status">{error ? (preview ? "原图加载失败，当前显示预览图" : "原图加载失败") : url ? "完整原图" : "正在加载完整原图…"}</span>{error && <Button onClick={() => setRevision(n => n + 1)}>重新加载</Button>}<Dialog.Close asChild><Button aria-label="关闭原图" size="icon"><X size={20} /></Button></Dialog.Close></header>
      <div className="rc-lightbox-stage" onClick={event => { if (event.currentTarget === event.target) onClose(); }}>{url || preview ? <img src={(url || preview)!} alt={`${title} 原图`} draggable={false} /> : error ? <p>无法加载这张图片。</p> : <span className="rc-image-loading" />}</div>
    </Dialog.Content>
  </Dialog.Portal></Dialog.Root>;
}
