import { useEffect, useLayoutEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { Dialog } from "radix-ui";
import { Download, Grip, Maximize, Minus, Plus, X } from "lucide-react";
import type { RowRecord } from "../../lib/api";
import { originalImages } from "../../lib/images/progressive-images";
import { rowFileName } from "../../lib/utils/row-display";
import { useImage, useProgressiveImage } from "./use-image";
import { Button } from "./controls";
import { beginFileDrag } from "../state/file-drag";
import { exportOriginalImage } from "../state/row-actions";
import "./detail-images.css";

export function DetailLightbox({ row, onClose }: { row: RowRecord; onClose: () => void }) {
  const original = useImage(originalImages, row.id, true);
  const preview = useProgressiveImage(row.id);
  const stage = useRef<HTMLDivElement>(null);
  const opener = useRef<HTMLElement | null>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [natural, setNatural] = useState({ width: row.imageWidth || 1, height: row.imageHeight || 1 });
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const drag = useRef<{ id: number; x: number; y: number; startX: number; startY: number } | null>(null);
  const [dragging, setDragging] = useState(false);
  const [decodeFailed, setDecodeFailed] = useState(false);
  const title = rowFileName(row) || `第 ${row.sourceOrdinal} 行图片`;
  const url = original.url || preview.url;
  const fit = Math.min(1, size.width / natural.width || 1, size.height / natural.height || 1);
  const width = natural.width * fit;
  const height = natural.height * fit;
  const clampPan = (x: number, y: number, factor = zoom) => ({ x: Math.max(-Math.max(0, (width * factor - size.width) / 2), Math.min(Math.max(0, (width * factor - size.width) / 2), x)), y: Math.max(-Math.max(0, (height * factor - size.height) / 2), Math.min(Math.max(0, (height * factor - size.height) / 2), y)) });
  useLayoutEffect(() => { opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null; }, []);
  useEffect(() => {
    const node = stage.current;
    if (!node) return;
    const observer = new ResizeObserver(entries => { const rect = entries[0]?.contentRect; if (rect) setSize({ width: rect.width, height: rect.height }); });
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  useEffect(() => { setDecodeFailed(false); }, [url]);
  useEffect(() => { setPan(current => clampPan(current.x, current.y)); }, [size.width, size.height, natural.width, natural.height]);
  const changeZoom = (next: number, point = { x: 0, y: 0 }) => {
    const factor = Math.max(1, Math.min(Math.max(8, 1 / fit), next));
    setPan(current => clampPan(point.x - (point.x - current.x) * factor / zoom, point.y - (point.y - current.y) * factor / zoom, factor));
    setZoom(factor);
  };
  const reset = () => { setZoom(1); setPan({ x: 0, y: 0 }); };
  const release = (event: ReactPointerEvent<HTMLDivElement>) => { if (drag.current?.id !== event.pointerId) return; drag.current = null; setDragging(false); if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); };
  return <Dialog.Root open onOpenChange={open => { if (!open) onClose(); }}><Dialog.Portal><Dialog.Overlay className="rd-lightbox-overlay" /><Dialog.Content className="rd-lightbox" aria-describedby={undefined} onCloseAutoFocus={event => { event.preventDefault(); opener.current?.focus(); }} onEscapeKeyDown={event => event.stopPropagation()} onKeyDown={event => { if (event.key === "+" || event.key === "=") { event.preventDefault(); changeZoom(zoom * 1.25); } if (event.key === "-") { event.preventDefault(); changeZoom(zoom / 1.25); } if (event.key === "0") { event.preventDefault(); reset(); } }}>
    <header className="rd-lightbox-head"><div><Dialog.Title title={title}>{title}</Dialog.Title><span role="status">{original.error ? "原图加载失败，当前显示预览图" : original.url ? "完整原图" : "正在加载完整原图…"}</span></div><div className="rd-lightbox-actions"><Button title="拖出原图到其他应用" aria-label="拖出原图到其他应用" onMouseDown={event => beginFileDrag(event.nativeEvent, row.id)}><Grip size={15} />拖出</Button><Button title="保存原图" aria-label="保存原图" onClick={() => void exportOriginalImage(row)}><Download size={15} /></Button><Dialog.Close asChild><Button size="icon" aria-label="关闭图片放大预览"><X size={18} /></Button></Dialog.Close></div></header>
    <div className="rd-lightbox-stage" ref={stage} data-zoomed={zoom > 1} data-dragging={dragging} onWheel={event => { event.preventDefault(); const rect = event.currentTarget.getBoundingClientRect(); changeZoom(zoom * (event.deltaY < 0 ? 1.15 : 1 / 1.15), { x: event.clientX - rect.left - rect.width / 2, y: event.clientY - rect.top - rect.height / 2 }); }} onDoubleClick={() => zoom > 1 ? reset() : changeZoom(1 / fit)} onPointerDown={event => { if (event.button !== 0 || zoom <= 1) return; event.preventDefault(); drag.current = { id: event.pointerId, x: event.clientX, y: event.clientY, startX: pan.x, startY: pan.y }; event.currentTarget.setPointerCapture(event.pointerId); setDragging(true); }} onPointerMove={event => { const origin = drag.current; if (origin?.id === event.pointerId) setPan(clampPan(origin.startX + event.clientX - origin.x, origin.startY + event.clientY - origin.y)); }} onPointerUp={release} onPointerCancel={release}>
      {url && !decodeFailed ? <img src={url} alt={title} draggable={false} onLoad={event => setNatural({ width: event.currentTarget.naturalWidth, height: event.currentTarget.naturalHeight })} onError={() => setDecodeFailed(true)} style={{ width, height, transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})` }} /> : <div className="rd-lightbox-empty">{original.error || decodeFailed ? "图片无法读取" : "正在加载图片…"}</div>}
    </div>
    <footer className="rd-lightbox-footer"><span>滚轮缩放 · 放大后拖动画面 · 双击切换原始尺寸</span>{original.error && <Button onClick={original.retry}>重试原图</Button>}<Button size="icon" aria-label="缩小图片" disabled={zoom <= 1} onClick={() => changeZoom(zoom / 1.25)}><Minus size={15} /></Button><span className="rd-zoom-value">{Math.round(fit * zoom * 100)}%</span><Button size="icon" aria-label="放大图片" onClick={() => changeZoom(zoom * 1.25)}><Plus size={15} /></Button><Button aria-label="适合窗口" onClick={reset}><Maximize size={14} />适合窗口</Button></footer>
  </Dialog.Content></Dialog.Portal></Dialog.Root>;
}
