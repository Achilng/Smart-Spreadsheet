import { useEffect, useLayoutEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { Dialog } from "radix-ui";
import { Download, Expand, Grip, ImageOff, LoaderCircle, Maximize, Minus, Plus, RotateCcw, Shrink, X } from "lucide-react";
import type { RowRecord } from "../../lib/api";
import { originalImages } from "../../lib/images/progressive-images";
import { clampImagePan, imageFitScale, MAX_IMAGE_SCALE, zoomImageAt, type ImagePoint } from "../../lib/images/lightbox-geometry";
import { rowFileName } from "../../lib/utils/row-display";
import { useImage, useProgressiveImage } from "./use-image";
import { Button } from "./controls";
import { beginFileDrag } from "../state/file-drag";
import { exportOriginalImage } from "../state/row-actions";
import "./detail-images.css";

interface DecodedImage { url: string; width: number; height: number }
function useDecodedImage(url: string | null) {
  const [result, setResult] = useState<{ url: string; image: DecodedImage | null; failed: boolean } | null>(null);
  useEffect(() => {
    if (!url) return;
    let disposed = false;
    const image = new Image(); image.src = url;
    void image.decode().then(() => {
      if (!disposed) setResult({ url, image: { url, width: image.naturalWidth, height: image.naturalHeight }, failed: false });
    }, () => { if (!disposed) setResult({ url, image: null, failed: true }); });
    return () => { disposed = true; };
  }, [url]);
  return result?.url === url ? result : null;
}
interface View { fitted: boolean; scale: number; pan: ImagePoint }
const fittedView = (): View => ({ fitted: true, scale: 1, pan: { x: 0, y: 0 } });

export function DetailLightbox({ row, onClose }: { row: RowRecord; onClose: () => void }) {
  const original = useImage(originalImages, row.id, true), preview = useProgressiveImage(row.id);
  const full = useDecodedImage(original.url), fallback = useDecodedImage(preview.url);
  const display = full?.image ?? fallback?.image;
  const [stage, setStage] = useState<HTMLDivElement | null>(null);
  const opener = useRef<HTMLElement | null>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [view, setView] = useState<View>(fittedView);
  const [expanded, setExpanded] = useState(false), [dragging, setDragging] = useState(false);
  const drag = useRef<{ id: number; x: number; y: number; pan: ImagePoint } | null>(null);
  const natural = full?.image ?? { width: row.imageWidth || display?.width || 1, height: row.imageHeight || display?.height || 1 };
  const fit = imageFitScale(natural, size);
  const scale = view.fitted ? fit : Math.max(fit, Math.min(MAX_IMAGE_SCALE, view.scale));
  const pan = view.fitted ? { x: 0, y: 0 } : clampImagePan(view.pan, natural, size, scale);
  const canPan = Boolean(display && scale > fit + .00001), ready = Boolean(display && fit > 0);
  const failed = original.error || Boolean(full?.failed);
  const title = rowFileName(row) || `第 ${row.sourceOrdinal} 行图片`;
  const status = failed ? (display ? "原图加载失败 · 当前为预览图" : "原图加载失败") : full?.image ? "完整原图" : "正在加载原图…";

  useLayoutEffect(() => { opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null; }, []);
  // The portal mounts after this component: observe the actual node, not an empty ref on the first effect.
  useLayoutEffect(() => {
    if (!stage) return;
    const measure = () => {
      const rect = stage.getBoundingClientRect();
      setSize(current => current.width === rect.width && current.height === rect.height ? current : { width: rect.width, height: rect.height });
    };
    measure();
    const observer = new ResizeObserver(measure); observer.observe(stage);
    return () => observer.disconnect();
  }, [stage]);

  function endDrag() {
    const pointer = drag.current; drag.current = null; setDragging(false);
    if (pointer && stage?.hasPointerCapture(pointer.id)) stage.releasePointerCapture(pointer.id);
  }
  useEffect(() => {
    endDrag();
    setView(current => current.fitted ? current : { ...current, pan: clampImagePan(current.pan, natural, size, Math.max(fit, current.scale)) });
  }, [size.width, size.height, natural.width, natural.height]);
  useEffect(() => {
    window.addEventListener("blur", endDrag);
    return () => window.removeEventListener("blur", endDrag);
  }, [stage]);

  function changeZoom(next: number | ((current: number) => number), point: ImagePoint = { x: 0, y: 0 }) {
    if (!ready) return;
    endDrag();
    setView(current => {
      const previous = current.fitted ? fit : Math.max(fit, Math.min(MAX_IMAGE_SCALE, current.scale));
      const factor = Math.max(fit, Math.min(MAX_IMAGE_SCALE, typeof next === "function" ? next(previous) : next));
      if (factor <= fit + .00001) return fittedView();
      const currentPan = current.fitted ? { x: 0, y: 0 } : clampImagePan(current.pan, natural, size, previous);
      return { fitted: false, scale: factor, pan: clampImagePan(zoomImageAt(currentPan, point, previous, factor), natural, size, factor) };
    });
  }
  const reset = () => { endDrag(); setView(fittedView()); };
  const wheelZoom = useRef<(event: WheelEvent) => void>(() => {});
  wheelZoom.current = event => {
    if (!ready || !event.deltaY || !stage) return;
    event.preventDefault();
    const rect = stage.getBoundingClientRect();
    const delta = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? rect.height : 1);
    changeZoom(current => current * Math.exp(-Math.max(-160, Math.min(160, delta)) * .0025), { x: event.clientX - rect.left - rect.width / 2, y: event.clientY - rect.top - rect.height / 2 });
  };
  useEffect(() => {
    if (!stage) return;
    const wheel = (event: WheelEvent) => wheelZoom.current(event);
    stage.addEventListener("wheel", wheel, { passive: false });
    return () => stage.removeEventListener("wheel", wheel);
  }, [stage]);
  function release(event: ReactPointerEvent<HTMLDivElement>) { if (drag.current?.id === event.pointerId) endDrag(); }
  function retry() { originalImages.clear(); original.retry(); preview.retry(); }

  return <Dialog.Root open onOpenChange={open => { if (!open) onClose(); }}><Dialog.Portal>
    <Dialog.Overlay className="rd-lightbox-overlay" />
    <Dialog.Content className="rd-lightbox" data-expanded={expanded} onOpenAutoFocus={event => { event.preventDefault(); (event.currentTarget as HTMLElement).querySelector<HTMLElement>(".rd-lightbox-stage")?.focus(); }} onCloseAutoFocus={event => { event.preventDefault(); opener.current?.focus(); }} onEscapeKeyDown={event => event.stopPropagation()} onKeyDown={event => {
      if (event.ctrlKey || event.metaKey || event.altKey || event.target instanceof HTMLInputElement) return;
      if (event.key === "+" || event.key === "=") { event.preventDefault(); changeZoom(value => value * 1.25); }
      else if (event.key === "-") { event.preventDefault(); changeZoom(value => value / 1.25); }
      else if (event.key === "0") { event.preventDefault(); reset(); }
      else if (event.key === "1") { event.preventDefault(); changeZoom(1); }
      else if (event.target === stage && canPan && ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) {
        event.preventDefault();
        const step = event.shiftKey ? 120 : 40;
        setView(current => ({ ...current, pan: clampImagePan({ x: pan.x + (event.key === "ArrowLeft" ? step : event.key === "ArrowRight" ? -step : 0), y: pan.y + (event.key === "ArrowUp" ? step : event.key === "ArrowDown" ? -step : 0) }, natural, size, scale) }));
      }
    }}>
      <Dialog.Description className="sr-only">滚轮以鼠标位置为中心缩放，放大后拖动查看。双击切换适合窗口与原始尺寸，方向键平移，0 适合窗口，1 原始尺寸，Esc 关闭。</Dialog.Description>
      <header className="rd-lightbox-head"><div className="rd-lightbox-info"><Dialog.Title title={title}>{title}</Dialog.Title><div className="rd-lightbox-metadata"><span>{display ? `${natural.width.toLocaleString()} × ${natural.height.toLocaleString()}` : "原图预览"}</span><span className="rd-lightbox-status" data-error={failed} role="status">{!failed && !full?.image && <LoaderCircle size={13} className="rd-lightbox-spinner" />}{status}</span>{failed && <Button size="sm" variant="ghost" onClick={retry}><RotateCcw size={13} />重试原图</Button>}</div></div>
        <div className="rd-lightbox-actions"><Button variant="ghost" title="拖出原图到其他应用" aria-label="拖出原图到其他应用" onMouseDown={event => beginFileDrag(event.nativeEvent, row.id)}><Grip size={16} /><span>拖出</span></Button><Button variant="ghost" title="保存原图" aria-label="保存原图" onClick={() => void exportOriginalImage(row)}><Download size={16} /><span>保存</span></Button><Button variant="ghost" size="icon" title={expanded ? "还原预览窗口" : "铺满窗口"} aria-label={expanded ? "还原预览窗口" : "铺满窗口"} onClick={() => setExpanded(value => !value)}>{expanded ? <Shrink size={17} /> : <Expand size={17} />}</Button><Dialog.Close asChild><Button variant="ghost" size="icon" title="关闭 · Esc" aria-label="关闭图片放大预览"><X size={20} /></Button></Dialog.Close></div>
      </header>
      <div className="rd-lightbox-stage" ref={setStage} tabIndex={0} role="region" aria-label="原图画布" data-zoomed={canPan} data-dragging={dragging}
        onDoubleClick={event => { const rect = event.currentTarget.getBoundingClientRect(); if (canPan) reset(); else changeZoom(1, { x: event.clientX - rect.left - rect.width / 2, y: event.clientY - rect.top - rect.height / 2 }); }}
        onPointerDown={event => { if (event.button !== 0 || !canPan) return; event.preventDefault(); event.currentTarget.focus({ preventScroll: true }); drag.current = { id: event.pointerId, x: event.clientX, y: event.clientY, pan }; event.currentTarget.setPointerCapture(event.pointerId); setDragging(true); }}
        onPointerMove={event => { const start = drag.current; if (start?.id === event.pointerId) setView(current => ({ ...current, pan: clampImagePan({ x: start.pan.x + event.clientX - start.x, y: start.pan.y + event.clientY - start.y }, natural, size, scale) })); }}
        onPointerUp={release} onPointerCancel={release} onLostPointerCapture={release}>
        {display ? <img src={display.url} alt={title} draggable={false} style={{ width: natural.width, height: natural.height, visibility: ready ? "visible" : "hidden", transform: `translate(-50%, -50%) translate3d(${pan.x}px, ${pan.y}px, 0) scale(${scale})` }} /> : <div className="rd-lightbox-empty">{failed ? <><ImageOff size={32} /><strong>暂时无法读取这张图片</strong><Button onClick={retry}><RotateCcw size={15} />重新加载</Button></> : <><LoaderCircle size={28} className="rd-lightbox-spinner" /><span>正在加载图片…</span></>}</div>}
      </div>
      <footer className="rd-lightbox-footer"><span className="rd-lightbox-help">滚轮缩放 · 拖动平移 · 双击切换 · Esc 关闭</span><div className="rd-lightbox-zoom" role="group" aria-label="图片缩放"><Button variant="ghost" size="icon" title="缩小 · −" aria-label="缩小图片" disabled={!ready || scale <= fit + .00001} onClick={() => changeZoom(value => value / 1.25)}><Minus size={17} /></Button><output className="rd-zoom-value" aria-label="当前缩放比例" aria-live="off">{ready ? `${Math.round(scale * 1000) / 10}%` : "—"}</output><Button variant="ghost" size="icon" title="放大 · +" aria-label="放大图片" disabled={!ready || scale >= MAX_IMAGE_SCALE} onClick={() => changeZoom(value => value * 1.25)}><Plus size={17} /></Button></div><div className="rd-lightbox-modes"><Button variant="ghost" aria-label="原始尺寸" title="原始尺寸 · 1" aria-pressed={ready && Math.abs(scale - 1) < .00001} disabled={!ready} onClick={() => changeZoom(1)}>100%</Button><Button variant="ghost" aria-label="适合窗口" title="适合窗口 · 0" aria-pressed={view.fitted} disabled={!ready} onClick={reset}><Maximize size={15} />适合窗口</Button></div></footer>
    </Dialog.Content>
  </Dialog.Portal></Dialog.Root>;
}
