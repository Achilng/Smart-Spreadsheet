import { useEffect, useRef, useState } from "react";
import { motion } from "motion/react";
import type { RowPage } from "../../lib/api";
import { errorText } from "../../lib/utils/format";
import { AlbumStack } from "./AlbumStack";
import { Thumbnail } from "./Thumbnail";
import { Button, RightClickMenu, type MenuItem } from "./controls";

let active = 0;
const waiting: (() => void)[] = [];
type Preview = Pick<RowPage, "rows" | "totalCount">;
// Keep the bookshelf ready while visiting an album. Source version is part of
// the key, so filters, edits and library switches never reuse stale covers.
const previews = new Map<string, Preview>();
function rememberPreview(key: string, page: Preview) {
  previews.delete(key);
  previews.set(key, { rows: page.rows.slice(0, 3), totalCount: page.totalCount });
  while (previews.size > 96) previews.delete(previews.keys().next().value!);
}
function limited<T>(run: () => Promise<T>): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const begin = () => { active++; void run().then(resolve, reject).finally(() => { active--; waiting.shift()?.(); }); };
    if (active < 4) begin(); else waiting.push(begin);
  });
}

/** Mount with a new key when the source query changes; keep loadPreview stable. */
export function RowAlbumCard({ albumKey, previewKey, label, count, actionLabel, loadPreview, initialPreview, onOpen, menu, layoutKey }: {
  layoutKey?: string;
  albumKey: string; previewKey: string; label: string; count?: number; actionLabel: string; loadPreview: () => Promise<RowPage>; initialPreview?: Preview; onOpen: () => void; menu?: MenuItem[];
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [page, setPage] = useState<Preview | null>(() => initialPreview ? { rows: initialPreview.rows.slice(0, 3), totalCount: initialPreview.totalCount } : previews.get(previewKey) ?? null);
  const [visible, setVisible] = useState(Boolean(page));
  const [error, setError] = useState(""), [attempt, setAttempt] = useState(0);
  useEffect(() => { if (page) rememberPreview(previewKey, page); }, [previewKey, page]);
  useEffect(() => {
    if (!ref.current) return;
    const observer = new IntersectionObserver(entries => setVisible(entries[0].isIntersecting), { root: ref.current.closest(".r-section-list"), rootMargin: "160px" });
    observer.observe(ref.current); return () => observer.disconnect();
  }, []);
  useEffect(() => {
    if (!visible || page || count === 0) return;
    let cancelled = false;
    setError("");
    void limited(async () => cancelled ? null : loadPreview()).then(result => {
      if (!cancelled && result) setPage(result);
    }, cause => { if (!cancelled) setError(errorText(cause)); });
    return () => { cancelled = true; };
  }, [visible, count, loadPreview, attempt, page]);
  const total = count ?? page?.totalCount;
  return <motion.div layout={layoutKey ? "position" : false} layoutDependency={layoutKey} initial={false} ref={ref} role="listitem" className="r-group-album" data-album-key={albumKey}>
    <RightClickMenu items={menu}><button className="r-album-trigger" type="button" aria-label={actionLabel} onClick={onOpen}>
      <AlbumStack images={visible ? (page?.rows ?? []).map(row => <Thumbnail key={row.id} rowId={row.id} hasImage={Boolean(row.imagePath?.trim() || row.storedImagePath?.trim())} alt={label} />) : []} empty={error ? "封面加载失败" : total === 0 ? "空相册" : !page ? "正在加载…" : "暂无图片"} />
      <span className="r-album-title" title={label}>{label}</span><span className="r-album-meta">{total === undefined ? "待整理的图片" : `${total.toLocaleString()} 张图片`}</span>
    </button></RightClickMenu>
    {error && <div className="r-group-cover-error"><span role="alert">{error}</span><Button size="sm" onClick={() => setAttempt(value => value + 1)}>重试封面</Button></div>}
  </motion.div>;
}
