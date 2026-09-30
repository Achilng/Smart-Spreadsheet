import { memo, useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { motion, MotionConfig } from "motion/react";
import { Star } from "lucide-react";
import { setFavorite, type RowRecord, type TagSummary } from "../../lib/api";
import { notify } from "../state/notices";
import { errorText } from "../../lib/utils/format";
import { notifyToolboxLibraryChanged } from "../../lib/windows/library-events";
import { galleryCellPosition, galleryLayout, galleryVisibleIndices } from "../../lib/images/gallery-layout";
import { clearFilters, ensurePage, PAGE_SIZE, patchRowFields, reloadRows, useLibrary, useRows } from "../state/library";
import { useMaterials } from "../state/materials";
import { useWorkspace } from "../state/workspace";
import { isSelected, selectedCount, toggleRow, useSelection } from "../state/selection";
import { Thumbnail } from "../ui/Thumbnail";
import { Button, Checkbox } from "../ui/controls";
import { rowFileName, rowResolution } from "../../lib/utils/row-display";
import { modelVersionBadge } from "../../lib/utils/model-version";
import { tagColorFor } from "../../lib/utils/tag-colors";
import { thumbnails } from "../ui/use-image";
import { useViewport } from "../ui/use-viewport";
import { useReducedMotionPreference } from "../ui/use-reduced-motion";
import { rememberVisibleRange } from "../../lib/stores/view-state";

import { RowContextMenu } from "../ui/RowContextMenu";
import { beginFileDrag } from "../state/file-drag";

const savingFavorites = new Set<string>();

function FavoriteButton({ row }: { row: RowRecord }) {
  const [saving, setSaving] = useState(false);
  const toggleFavorite = async () => {
    const directory = useLibrary.getState().snapshot?.dataDirectory;
    const key = `${directory}:${row.id}`;
    if (savingFavorites.has(key)) return;
    savingFavorites.add(key);
    setSaving(true);
    try {
      const favorite = !row.favorite;
      if (await setFavorite(row.id, favorite) === 0) throw new Error("图片已不存在");
      if (useLibrary.getState().snapshot?.dataDirectory !== directory) return;
      patchRowFields(row.id, { favorite }, { resetScroll: false });
      notifyToolboxLibraryChanged("main");
    } catch (error) {
      notify(`收藏更新失败：${errorText(error)}`, "error");
    } finally {
      savingFavorites.delete(key);
      setSaving(false);
    }
  };
  return <button type="button" className="r-card-favorite" aria-label={`${row.favorite ? "取消收藏" : "收藏"}第 ${row.sourceOrdinal} 行`} title={row.favorite ? "取消收藏" : "收藏"} aria-pressed={row.favorite} disabled={saving} onMouseDown={event => event.stopPropagation()} onDoubleClick={event => event.stopPropagation()} onClick={event => { event.stopPropagation(); void toggleFavorite(); }}>
    <Star size={20} strokeWidth={1.8} fill={row.favorite ? "currentColor" : "none"} aria-hidden="true" />
  </button>;
}

// Geometry changes every animation frame. Keep the interactive card subtree
// stable while its lightweight outer frame follows the available gallery width.
const GalleryCardContent = memo(function GalleryCardContent({ row, index, checked, active, pickingMaterial, tags, layoutKey }: {
  row: RowRecord; index: number; checked: boolean; active: boolean; pickingMaterial: boolean; tags: TagSummary[]; layoutKey: string;
}) {
  const dragged = useRef(false);
  const badge = modelVersionBadge(row.generationModel);
  return <RowContextMenu row={row}><div className="r-card-content" onContextMenu={() => useRows.setState({ activeRow: row })}>
    {!pickingMaterial && <Checkbox aria-label={`选择第 ${row.sourceOrdinal} 行`} className="r-card-checkbox" checked={checked} onClick={event => toggleRow(row.id, index, event.shiftKey)} />}
    <FavoriteButton row={row} />
    <button type="button" className="r-thumb" aria-label={`查看第 ${row.sourceOrdinal} 行详情`} aria-pressed={active} onMouseDown={event => { dragged.current = false; if (row.imagePath || row.storedImagePath) beginFileDrag(event.nativeEvent, row.id, () => { dragged.current = true; }); }} onClick={event => {
      if (dragged.current) { dragged.current = false; return; }
      if (!pickingMaterial && (event.ctrlKey || event.metaKey || (event.shiftKey && useSelection.getState().anchor !== null))) toggleRow(row.id, index, event.shiftKey);
      else useRows.setState({ activeRow: row });
    }}>
      <Thumbnail enhanced hasImage={Boolean(row.imagePath || row.storedImagePath)} rowId={row.id} alt={`第 ${row.sourceOrdinal} 行缩略图`} />
      {row.tags.length > 0 && <span className="r-card-tags" title={row.tags.join("、")}>{row.tags.slice(0, 2).map(tag => {
        const tone = tagColorFor(tag, tags); return <span key={tag} style={{ background: tone.background, color: tone.text }}>{tag}</span>;
      })}{row.tags.length > 2 && <span className="r-tag-more">+{row.tags.length - 2}</span>}</span>}
    </button>
    <motion.div layout="position" layoutDependency={layoutKey} className="r-card-meta"><div title={rowFileName(row) ?? undefined}>{row.artistRepresentative && <span className="r-representative-label">代表图 · </span>}{rowFileName(row) ?? `#${row.sourceOrdinal}`}</div><small><span>{rowResolution(row) ?? `#${row.sourceOrdinal}`}</span>{(badge || !!row.vibeReferenceCount) && <span className="r-card-badges">{badge && <span className={`version-badge ${badge.className}`} title={`作画模型：${row.generationModel}`}>{badge.label}</span>}{!!row.vibeReferenceCount && <span className="vibe-badge" title={`包含 ${row.vibeReferenceCount} 个 VIBE 引用`}>VIBE ×{row.vibeReferenceCount}</span>}</span>}</small></motion.div>
  </div></RowContextMenu>;
});

export function Gallery() {
  const reducedMotion = useReducedMotionPreference();
  const pickingMaterial = useMaterials(state => !!state.galleryPick);
  const { viewport, size, onScroll } = useViewport("gallery");
  const cardSize = useWorkspace(state => state.galleryCardSize);
  const pages = useRows(state => state.pages);
  const total = useRows(state => state.total);
  const loading = useRows(state => state.loading);
  const refreshing = useRows(state => state.refreshing);
  const error = useRows(state => state.error);
  const activeId = useRows(state => state.activeRow?.id);
  const selection = useSelection();
  const tags = useLibrary(state => state.tags);
  const layout = galleryLayout(size.width, cardSize, total);
  const indices = galleryVisibleIndices(layout, size.top, size.height, total);
  const pageKey = [...new Set(indices.map(index => Math.floor(index / PAGE_SIZE)))].join(",");
  const selectionActive = !pickingMaterial && selectedCount(selection) > 0;
  const first = indices[0] ?? 0;
  const last = indices[indices.length - 1] ?? -1;
  useLayoutEffect(() => { if (!loading && !refreshing && last >= first) rememberVisibleRange("gallery", first, last); }, [first, last, loading, refreshing]);
  useEffect(() => {
    if (!pageKey || refreshing || loading) return;
    for (const page of pageKey.split(",").map(Number)) void ensurePage(page);
  }, [pageKey, refreshing, loading]);
  const visibleIds = indices.map(index => pages.get(Math.floor(index / PAGE_SIZE))?.[index % PAGE_SIZE]?.id).filter((id): id is number => id !== undefined).join(",");
  useEffect(() => { thumbnails.retain(new Set(visibleIds ? visibleIds.split(",").map(Number) : [])); }, [visibleIds]);
  // Motion caches its own OS preference at mount. Supply a live duration instead,
  // including when the user changes the setting while the gallery is open.
  return <MotionConfig reducedMotion="never" transition={{ layout: { duration: reducedMotion ? 0 : .28, ease: [.22, 1, .36, 1] } }}><motion.div layoutScroll className="r-gallery" ref={viewport} role="list" tabIndex={0} aria-label="图片画廊" aria-busy={loading || refreshing} onScroll={onScroll}>
    {loading ? <div className="r-state-message" role="status">正在加载图片…</div> : error && total === 0 ? <div className="r-state-message"><p>{error}</p><Button onClick={() => void reloadRows()}>重试</Button></div>
      : total === 0 ? <div className="r-state-message"><p>没有符合条件的图片</p><Button variant="ghost" onClick={clearFilters}>清除筛选</Button></div>
      : <div className="r-gallery-spacer" style={{ height: layout.spacerHeight }}>{indices.map(index => {
        const row = pages.get(Math.floor(index / PAGE_SIZE))?.[index % PAGE_SIZE];
        const position = galleryCellPosition(index, layout);
        const style = { left: position.x, top: position.y, width: layout.cardWidth, "--image-height": `${layout.imageHeight}px` } as CSSProperties;
        if (!row) return <div key={`placeholder-${index}`} className="r-card r-card-skeleton" style={style}><div className="r-thumb r-image-placeholder" /></div>;
        const checked = !pickingMaterial && isSelected(row.id, selection);
        // Measure only real rearrangements. Sidebar width changes between column
        // boundaries still resize cards live without restarting a layout animation.
        const layoutKey = `${layout.columns}:${index}:${reducedMotion}`;
        return <motion.div key={row.id} layout layoutDependency={layoutKey} initial={false} role="listitem" className="r-card" data-active={activeId === row.id} data-checked={checked} data-selecting={selectionActive} style={style}>
          <GalleryCardContent row={row} index={index} checked={checked} active={activeId === row.id} pickingMaterial={pickingMaterial} tags={tags} layoutKey={layoutKey} />
        </motion.div>;
      })}</div>}
    {error && total > 0 && <div className="r-inline-error" role="alert"><span>{error}</span><Button size="sm" onClick={() => void reloadRows()}>重试</Button></div>}
  </motion.div></MotionConfig>;
}
