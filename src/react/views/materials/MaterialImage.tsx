import { useEffect, useRef, useState } from "react";
import type { ImageLoader } from "../../../lib/images/image-loader";
import { useImage } from "../../ui/use-image";
import { Button, Modal } from "../../ui/controls";

export function MaterialImage({ id, loader, alt, expandable = false, active = true }: { id: number; loader: ImageLoader; alt: string; expandable?: boolean; active?: boolean }) {
  const container = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(!expandable);
  const [lightbox, setLightbox] = useState(false);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => { if (!active) setLightbox(false); }, [active]);
  useEffect(() => {
    if (!expandable || !container.current) return;
    const observer = new IntersectionObserver(entries => setVisible(entries[0].isIntersecting), { rootMargin: "120px" });
    observer.observe(container.current); return () => observer.disconnect();
  }, [expandable]);
  return <div ref={container} className="rm-image"><LoadedImage key={`${id}-${attempt}`} id={active && (visible || lightbox) ? id : undefined} loader={loader} alt={alt} onOpen={expandable ? () => setLightbox(true) : undefined} onRetry={() => setAttempt(value => value + 1)} lightbox={active && lightbox} onClose={() => setLightbox(false)} /></div>;
}
function LoadedImage({ id, loader, alt, onOpen, onRetry, lightbox, onClose }: { id?: number; loader: ImageLoader; alt: string; onOpen?: () => void; onRetry: () => void; lightbox: boolean; onClose: () => void }) {
  const image = useImage(loader, id);
  return <>{image.url ? onOpen ? <button className="rm-image-button" onClick={onOpen} aria-label={`放大查看 ${alt}`}><img src={image.url} alt={alt} draggable={false} /></button> : <img src={image.url} alt={alt} draggable={false} /> : image.error ? <div className="rm-image-error">图片无法读取<Button size="sm" onClick={onRetry}>重试</Button></div> : <div className="r-image-placeholder" aria-label="正在加载图片" />}
    {onOpen && <Modal open={lightbox} onClose={onClose} title={alt} width={1000}><img className="rm-lightbox" src={image.url ?? undefined} alt={alt} /></Modal>}</>;
}
