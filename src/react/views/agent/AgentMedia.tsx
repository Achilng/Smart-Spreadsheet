import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Dialog } from "radix-ui";
import { ChevronLeft, ChevronRight, ImageOff, X } from "lucide-react";
import { isImageLoadCancelled, type ImageLoader } from "../../../lib/images/image-loader";
import { galleryPreviews } from "../../../lib/images/progressive-images";
import { thumbnails } from "../../../lib/images/thumbnails";

/** Object URL of a row image, loaded while the component is mounted. */
function useRowImage(loader: ImageLoader, id: number, enabled = true): { url: string | null; failed: boolean } {
  const [state, setState] = useState(() => ({ id, url: loader.cached(id), failed: false }));
  const current = state.id === id ? state : { id, url: loader.cached(id), failed: false };
  useEffect(() => {
    if (!enabled) return;
    const cached = loader.cached(id);
    if (cached) { setState({ id, url: cached, failed: false }); return; }
    const controller = new AbortController();
    loader.load(id, false, controller.signal)
      .then(url => setState({ id, url, failed: false }))
      .catch(error => { if (!isImageLoadCancelled(error)) setState({ id, url: null, failed: true }); });
    return () => controller.abort();
  }, [loader, id, enabled]);
  return current;
}

export function RowThumb({ id, size = 56, onOpen, label }: { id: number; size?: number; onOpen?: () => void; label?: string }) {
  const { url, failed } = useRowImage(thumbnails, id);
  const content = url ? <img src={url} alt="" draggable={false} /> : failed ? <ImageOff className="icon sm" aria-hidden="true" /> : null;
  const style = { width: size, height: size };
  const title = label ?? `#${id}`;
  if (!onOpen) return <span className={`ag-thumb${url ? "" : " is-empty"}`} style={style} title={title}>{content}</span>;
  return <button type="button" className={`ag-thumb${url ? "" : " is-empty"}`} style={style} title={title} aria-label={`查看图片 ${title}`} onClick={onOpen}>{content}</button>;
}

/** A row of thumbnails; the last tile turns into "+N" when there are more than fit. */
export function ThumbStrip({ ids, total = ids.length, max = 10, size = 56, onOpen }: {
  ids: number[]; total?: number; max?: number; size?: number; onOpen?: (index: number) => void;
}) {
  const overflow = total > max;
  const shown = ids.slice(0, overflow ? max - 1 : max);
  const rest = total - shown.length;
  return <div className="ag-strip">
    {shown.map((id, index) => <RowThumb key={id} id={id} size={size} onOpen={onOpen && (() => onOpen(index))} />)}
    {overflow && <button type="button" className="ag-thumb ag-thumb-more" style={{ width: size, height: size }} disabled={!onOpen || shown.length >= ids.length}
      title={`还有 ${rest} 张`} onClick={() => onOpen?.(shown.length)}>+{rest}</button>}
  </div>;
}

export interface ViewerRequest {
  title: string;
  ids: number[];
  index: number;
  /** Shown under the image, e.g. what will change in it. */
  detail?: ReactNode;
  /** Total rows behind `ids` when only the first ones were loaded. */
  total?: number;
}

const ViewerContext = createContext<(request: ViewerRequest) => void>(() => {});
export const useImageViewer = () => useContext(ViewerContext);

export function ImageViewerProvider({ children }: { children: ReactNode }) {
  const [request, setRequest] = useState<ViewerRequest | null>(null);
  const open = useCallback((next: ViewerRequest) => setRequest(next), []);
  return <ViewerContext.Provider value={open}>
    {children}
    <ImageViewer request={request} onClose={() => setRequest(null)} />
  </ViewerContext.Provider>;
}

function LargeImage({ id }: { id: number }) {
  const thumb = useRowImage(thumbnails, id);
  const large = useRowImage(galleryPreviews, id);
  const url = large.url ?? thumb.url;
  if (!url) return <div className="ag-viewer-missing">{large.failed && thumb.failed ? <><ImageOff className="icon" />图片读取不到</> : <span className="spinner" aria-label="加载中" />}</div>;
  return <img key={id} src={url} alt={`图片 #${id}`} draggable={false} />;
}

function ImageViewer({ request, onClose }: { request: ViewerRequest | null; onClose: () => void }) {
  const [index, setIndex] = useState(0);
  const film = useRef<HTMLDivElement>(null);
  useEffect(() => { if (request) setIndex(request.index); }, [request]);
  const ids = useMemo(() => request?.ids ?? [], [request]);
  const id = ids[Math.min(index, ids.length - 1)];
  const step = useCallback((delta: number) => setIndex(value => Math.max(0, Math.min(ids.length - 1, value + delta))), [ids.length]);
  useEffect(() => {
    film.current?.querySelector<HTMLElement>(`[data-index="${index}"]`)?.scrollIntoView({ block: "nearest", inline: "center" });
  }, [index]);
  const hidden = (request?.total ?? ids.length) - ids.length;
  return <Dialog.Root open={!!request} onOpenChange={next => { if (!next) onClose(); }}><Dialog.Portal>
    <Dialog.Overlay className="ag-viewer-overlay" />
    <Dialog.Content className="ag-viewer" aria-describedby={undefined}
      onKeyDown={event => {
        if (event.key === "ArrowLeft") { event.preventDefault(); step(-1); }
        if (event.key === "ArrowRight") { event.preventDefault(); step(1); }
      }}>
      <header className="ag-viewer-head">
        <Dialog.Title className="ag-viewer-title">{request?.title}</Dialog.Title>
        {id !== undefined && <span className="ag-viewer-count">#{id}{ids.length > 1 && ` · 第 ${index + 1} / ${ids.length} 张`}</span>}
        <Dialog.Close asChild><button type="button" className="ag-viewer-btn" aria-label="关闭"><X className="icon" /></button></Dialog.Close>
      </header>
      <div className="ag-viewer-stage">
        {ids.length > 1 && <button type="button" className="ag-viewer-btn ag-viewer-nav is-prev" aria-label="上一张" disabled={index === 0} onClick={() => step(-1)}><ChevronLeft className="icon" /></button>}
        {id !== undefined && <LargeImage id={id} />}
        {ids.length > 1 && <button type="button" className="ag-viewer-btn ag-viewer-nav is-next" aria-label="下一张" disabled={index >= ids.length - 1} onClick={() => step(1)}><ChevronRight className="icon" /></button>}
      </div>
      {request?.detail && <div className="ag-viewer-detail">{request.detail}</div>}
      {ids.length > 1 && <div className="ag-viewer-film" ref={film}>
        {ids.map((item, position) => <button key={item} type="button" data-index={position} className={`ag-viewer-frame${position === index ? " is-active" : ""}`} aria-label={`图片 #${item}`} aria-current={position === index || undefined} onClick={() => setIndex(position)}>
          <RowThumb id={item} size={52} />
        </button>)}
        {hidden > 0 && <span className="ag-viewer-more">还有 {hidden} 张，可在主窗口查看</span>}
      </div>}
    </Dialog.Content>
  </Dialog.Portal></Dialog.Root>;
}
