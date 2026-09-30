import { memo, useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { motion, MotionConfig } from "motion/react";
import { materialAlbum } from "../../../lib/utils/material-album";
import { GALLERY_GAP, GALLERY_PADDING, GALLERY_PADDING_TOP, galleryLayout, galleryVisibleIndices } from "../../../lib/images/gallery-layout";
import { useMaterials, loadMaterialPage, MATERIAL_PAGE_SIZE, materialThumbnails, materialCardVersionImages } from "../../state/materials";
import { useReducedMotionPreference } from "../../ui/use-reduced-motion";
import { MaterialCard } from "./MaterialCard";

function materialLayout(width: number, size: number, total: number) {
  const layout = galleryLayout(width, size, total);
  layout.imageHeight = Math.ceil(Math.min(size * .65, layout.cardWidth * .58) * 1216 / 832) + 32;
  layout.cellHeight = layout.imageHeight + 110;
  return layout;
}

function applyGeometry(node: HTMLElement, layout: ReturnType<typeof materialLayout>) {
  node.style.setProperty("--material-card-width", `${layout.cardWidth}px`);
  node.style.setProperty("--album-stage-height", `${layout.imageHeight}px`);
  node.style.setProperty("--material-cell-height", `${layout.cellHeight}px`);
}

// Resize observations belong to the grid; sidebar controls and details stay stable.
export const MaterialGrid = memo(function MaterialGrid({ active, size }: { active: boolean; size: number }) {
  const state = useMaterials();
  const reducedMotion = useReducedMotionPreference();
  const viewport = useRef<HTMLDivElement>(null);
  const bounds = useRef({ width: 0, height: 0 });
  const [, updateWindow] = useState(0);
  const config = useRef({ active, size, total: state.total });
  const layout = materialLayout(bounds.current.width, size, state.total);
  const visible = active ? galleryVisibleIndices(layout, state.scrollTop, bounds.current.height, state.total) : [];
  const first = visible[0] ?? 0; const last = visible.at(-1) ?? 0;
  const windowKey = useRef("");
  useLayoutEffect(() => {
    const node = viewport.current;
    if (!node) return;
    const observer = new ResizeObserver(entries => {
      const { width, height } = entries[0].contentRect;
      bounds.current = { width, height };
      const current = config.current;
      const nextLayout = materialLayout(width, current.size, current.total);
      const next = current.active ? galleryVisibleIndices(nextLayout, useMaterials.getState().scrollTop, height, current.total) : [];
      const nextKey = `${nextLayout.columns}:${next[0] ?? 0}:${next.at(-1) ?? 0}:${height}`;
      // Keep continuous resizing without rendering controls or Motion nodes every pixel.
      // A column/window change must wait for React so Motion can snapshot the old layout.
      if (nextKey !== windowKey.current) updateWindow(value => value + 1);
      else applyGeometry(node, nextLayout);
    });
    observer.observe(node);
    node.scrollTop = useMaterials.getState().scrollTop;
    return () => observer.disconnect();
  }, []);
  useLayoutEffect(() => { if (viewport.current && viewport.current.scrollTop !== state.scrollTop) viewport.current.scrollTop = state.scrollTop; }, [state.scrollTop, active, bounds.current.height]);
  useLayoutEffect(() => {
    config.current = { active, size, total: state.total };
    windowKey.current = `${layout.columns}:${first}:${last}:${bounds.current.height}`;
    if (viewport.current) applyGeometry(viewport.current, layout);
  });
  const gridStyle = {
    "--album-cover-max": `${size * .65}px`,
    height: `calc(${layout.gridRows} * var(--material-cell-height) + ${GALLERY_PADDING_TOP + GALLERY_PADDING - GALLERY_GAP}px)`,
  } as CSSProperties;
  useEffect(() => {
    if (!active) return;
    const ids = new Set<number>();
    const versionIds = new Set<number>();
    for (let page = Math.floor(first / MATERIAL_PAGE_SIZE); page <= Math.floor(last / MATERIAL_PAGE_SIZE); page++) void loadMaterialPage(page);
    for (const index of visible) {
      const item = state.pages.get(Math.floor(index / MATERIAL_PAGE_SIZE))?.[index % MATERIAL_PAGE_SIZE];
      if (!item) continue;
      ids.add(item.id);
      for (const image of materialAlbum(item, state.cardVersions[item.id]).previews) if (image.kind === "version") versionIds.add(image.id);
    }
    materialThumbnails.retain(ids);
    materialCardVersionImages.retain(versionIds);
  }, [first, last, state.pages, state.cardVersions, active]);
  return <MotionConfig reducedMotion="never" transition={{ layout: { duration: reducedMotion ? 0 : .28, ease: [.22, 1, .36, 1] } }}><motion.div layoutScroll className="rm-viewport" ref={viewport} aria-busy={state.loading} onScroll={event => { if (active) useMaterials.setState({ scrollTop: event.currentTarget.scrollTop }); }} tabIndex={0} aria-label="素材列表">
        {state.total > 0 ? <div className="rm-grid" style={gridStyle}>{visible.map(index => { const item = state.pages.get(Math.floor(index / MATERIAL_PAGE_SIZE))?.[index % MATERIAL_PAGE_SIZE]; return <motion.div layout="position" layoutDependency={`${layout.columns}:${index}:${reducedMotion}`} initial={false} key={item ? `material-${item.id}-${state.revision}` : `placeholder-${index}-${state.revision}`} className="rm-cell" style={{ left: `calc(${GALLERY_PADDING}px + ${index % layout.columns} * (var(--material-card-width) + ${GALLERY_GAP}px))`, top: `calc(${GALLERY_PADDING_TOP}px + ${Math.floor(index / layout.columns)} * var(--material-cell-height))` }}>{item ? <MaterialCard material={item} active={state.selected?.id === item.id} open={active && !state.editorOpen && !state.pendingDelete && state.openCardId === item.id} /> : <div className="r-image-placeholder" />}</motion.div>; })}</div> : <div className="rm-empty"><h2>{state.loading ? "正在读取素材…" : state.error ? "素材读取失败" : state.search || state.selectedTags.length || state.untagged ? "没有匹配的素材" : "收藏你的第一份素材"}</h2>{!state.loading && !state.error && <p>新建素材可从图库选图，也可以将本地图片拖到这里导入。</p>}</div>}
      </motion.div></MotionConfig>;
});
