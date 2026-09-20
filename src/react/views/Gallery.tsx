import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { galleryCellPosition, galleryLayout, galleryVisibleIndices } from "../../lib/images/gallery-layout";
import { clearFilters, ensurePage, PAGE_SIZE, reloadRows, useRows } from "../state/library";
import { useWorkspace } from "../state/workspace";
import { isSelected, selectedCount, toggleRow, useSelection } from "../state/selection";
import { Thumbnail } from "../ui/Thumbnail";
import { Button, Checkbox } from "../ui/controls";
import { rowFileName, rowResolution } from "../../lib/utils/row-display";
import { modelVersionBadge } from "../../lib/utils/model-version";
import { tagColorFor } from "../../lib/utils/tag-colors";
import { useLibrary } from "../state/library";
import { thumbnails } from "../ui/use-image";

export function Gallery() {
  const viewport = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 0, height: 0, top: 0 });
  const cardSize = useWorkspace(state => state.galleryCardSize);
  const pages = useRows(state => state.pages);
  const total = useRows(state => state.total);
  const loading = useRows(state => state.loading);
  const refreshing = useRows(state => state.refreshing);
  const error = useRows(state => state.error);
  const resetToken = useRows(state => state.resetToken);
  const activeId = useRows(state => state.activeRow?.id);
  const selection = useSelection();
  const tags = useLibrary(state => state.tags);
  const layout = galleryLayout(size.width, cardSize, total);
  const indices = galleryVisibleIndices(layout, size.top, size.height, total);
  const pageKey = [...new Set(indices.map(index => Math.floor(index / PAGE_SIZE)))].join(",");
  const selectionActive = selectedCount(selection) > 0;
  useLayoutEffect(() => {
    const node = viewport.current;
    if (!node) return;
    const measure = () => setSize(previous => ({ ...previous, width: node.clientWidth, height: node.clientHeight }));
    const observer = new ResizeObserver(measure);
    observer.observe(node); measure();
    return () => observer.disconnect();
  }, []);
  useLayoutEffect(() => {
    if (viewport.current) viewport.current.scrollTop = 0;
    setSize(previous => ({ ...previous, top: 0 }));
  }, [resetToken]);
  useEffect(() => {
    if (!pageKey || refreshing || loading) return;
    for (const page of pageKey.split(",").map(Number)) void ensurePage(page);
  }, [pageKey, refreshing, loading]);
  const visibleIds = indices.map(index => pages.get(Math.floor(index / PAGE_SIZE))?.[index % PAGE_SIZE]?.id).filter((id): id is number => id !== undefined).join(",");
  useEffect(() => { thumbnails.retain(new Set(visibleIds ? visibleIds.split(",").map(Number) : [])); }, [visibleIds]);
  return <div className="r-gallery" ref={viewport} role="list" tabIndex={0} aria-label="图片画廊" aria-busy={loading || refreshing} onScroll={event => {
    const top = event.currentTarget.scrollTop; setSize(previous => ({ ...previous, top }));
  }}>
    {loading ? <div className="r-state-message" role="status">正在加载图片…</div> : error && total === 0 ? <div className="r-state-message"><p>{error}</p><Button onClick={() => void reloadRows()}>重试</Button></div>
      : total === 0 ? <div className="r-state-message"><p>没有符合条件的图片</p><Button variant="ghost" onClick={clearFilters}>清除筛选</Button></div>
      : <div className="r-gallery-spacer" style={{ height: layout.spacerHeight }}>{indices.map(index => {
        const row = pages.get(Math.floor(index / PAGE_SIZE))?.[index % PAGE_SIZE];
        const position = galleryCellPosition(index, layout);
        const style = { left: position.x, top: position.y, width: layout.cardWidth, "--image-height": `${layout.imageHeight}px` } as CSSProperties;
        if (!row) return <div key={index} className="r-card r-card-skeleton" style={style}><div className="r-thumb r-image-placeholder" /></div>;
        const badge = modelVersionBadge(row.generationModel);
        const checked = isSelected(row.id, selection);
        return <div key={row.id} role="listitem" className="r-card" data-active={activeId === row.id} data-checked={checked} data-selecting={selectionActive} style={style}>
          <Checkbox aria-label={`选择第 ${row.sourceOrdinal} 行`} className="r-card-checkbox" checked={checked} onClick={event => toggleRow(row.id, index, event.shiftKey)} />
          <button type="button" className="r-thumb" aria-label={`查看第 ${row.sourceOrdinal} 行详情`} aria-pressed={activeId === row.id} onClick={event => {
            if (event.ctrlKey || event.metaKey || (event.shiftKey && selection.anchor !== null)) toggleRow(row.id, index, event.shiftKey);
            else useRows.setState({ activeRow: row });
          }}>
            <Thumbnail rowId={row.id} alt={`第 ${row.sourceOrdinal} 行缩略图`} />
            {badge && <span className={`r-model-badge version-badge ${badge.className}`}>{badge.label}</span>}
            {!!row.vibeReferenceCount && <span className="r-vibe-badge">VIBE ×{row.vibeReferenceCount}</span>}
            {row.tags.length > 0 && <span className="r-card-tags" title={row.tags.join("、")}>{row.tags.slice(0, 2).map(tag => {
              const tone = tagColorFor(tag, tags); return <span key={tag} style={{ background: tone.background, color: tone.text }}>{tag}</span>;
            })}{row.tags.length > 2 && <span className="r-tag-more">+{row.tags.length - 2}</span>}</span>}
          </button>
          <div className="r-card-meta"><div title={rowFileName(row) ?? undefined}>{rowFileName(row) ?? `#${row.sourceOrdinal}`}</div><small>{rowResolution(row) ?? `#${row.sourceOrdinal}`}</small></div>
        </div>;
      })}</div>}
    {error && total > 0 && <div className="r-inline-error" role="alert"><span>{error}</span><Button size="sm" onClick={() => void reloadRows()}>重试</Button></div>}
  </div>;
}
