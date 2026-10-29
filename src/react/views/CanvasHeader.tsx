import { Grid2X2, SlidersHorizontal, X } from "lucide-react";
import { forwardRef, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { AnimatePresence, motion, useIsPresent } from "motion/react";
import { clearFilters, setQuery, useRows } from "../state/library";
import { useWorkspace } from "../state/workspace";
import { viewLabel } from "../../lib/utils/view-modes";
import { formatCount } from "../../lib/utils/format";
import { Button, Slider } from "../ui/controls";
import { ImageSortMenu } from "../ui/ImageSortMenu";
import { libraryFilterLabel } from "../../lib/utils/library-filters";
import { useGroups } from "../state/groups";
import { GalleryMotion, galleryTransition } from "../ui/GalleryMotion";
import { useReducedMotionPreference } from "../ui/use-reduced-motion";

type Chip = { key: string; label: string; remove: () => void };
const FilterChip = forwardRef<HTMLSpanElement, { chip: Chip; reducedMotion: boolean }>(function FilterChip({ chip, reducedMotion }, ref) {
  const present = useIsPresent();
  return <motion.span ref={ref} layout="position" className="r-filter-chip" inert={!present} aria-hidden={!present}
    initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}
    transition={{ ...galleryTransition, duration: reducedMotion ? 0 : galleryTransition.duration }}>
    <span title={chip.label}>{chip.label}</span><Button variant="ghost" size="icon" aria-label={`移除筛选 ${chip.label}`} onClick={chip.remove}><X size={12} /></Button>
  </motion.span>;
});

/** Move the content as a composited layer; never resize a virtual list per frame. */
export function FilterLayout({ children }: { children: ReactNode }) {
  const query = useRows(state => state.query);
  const groups = useGroups(state => state.list);
  const reducedMotion = useReducedMotionPreference();
  const content = useRef<HTMLDivElement>(null);
  const viewportContent = useRef<HTMLDivElement>(null);
  const movement = useRef<Animation | null>(null);
  const [reservedHeight, setReservedHeight] = useState(0);
  const targetHeight = useRef<number | null>(null);
  const occurrences = new Map<string, number>();
  const chips: Chip[] = [
    ...(query.search ? [{ key: "search", label: `“${query.search}”`, remove: () => setQuery({ search: "" }) }] : []),
    ...query.tags.map(tag => ({ key: `tag:${tag}`, label: tag, remove: () => setQuery({ tags: query.tags.filter(value => value !== tag) }) })),
    ...(query.dedupe !== "none" ? [{ key: "dedupe", label: query.dedupe === "artists" ? "按画师串去重" : "按正向去重", remove: () => setQuery({ dedupe: "none" }) }] : []),
    ...(query.singleArtistOnly ? [{ key: "singleArtistOnly", label: "单画师", remove: () => setQuery({ singleArtistOnly: false }) }] : []),
    ...(query.artistFilter ? [{ key: "artistFilter", label: `画师串：${query.artistFilter}`, remove: () => setQuery({ artistFilter: "" }) }] : []),
    ...(query.hasVibe ? [{ key: "hasVibe", label: "包含 VIBE", remove: () => setQuery({ hasVibe: false }) }] : []),
    ...(query.untaggedOnly ? [{ key: "untaggedOnly", label: "无 Tag", remove: () => setQuery({ untaggedOnly: false }) }] : []),
    ...(query.hideGrouped ? [{ key: "hideGrouped", label: "隐藏已分组", remove: () => setQuery({ hideGrouped: false }) }] : []),
    ...query.filters.map((filter, index) => {
      const value = JSON.stringify(filter), occurrence = occurrences.get(value) ?? 0;
      occurrences.set(value, occurrence + 1);
      return { key: `filter:${value}:${occurrence}`, label: libraryFilterLabel(filter, groups), remove: () => setQuery({ filters: query.filters.filter((_, cursor) => cursor !== index) }) };
    }),
  ];
  const contentKey = JSON.stringify(chips.map(({ key, label }) => [key, label]));
  useLayoutEffect(() => {
    const node = content.current, viewport = viewportContent.current;
    if (!node || !viewport) return;
    const measure = () => {
      const next = node.offsetHeight;
      if (next === targetHeight.current && !reducedMotion) return;
      const transform = `translateY(${next}px)`;
      const from = getComputedStyle(viewport).transform;
      const immediate = targetHeight.current === null || reducedMotion;
      movement.current?.cancel();
      movement.current = null;
      viewport.style.transform = transform;
      targetHeight.current = next;
      // Keep enough viewport height for both ends of the motion. The outer
      // layout clips the excess at the bottom, so no blank strip is exposed.
      if (immediate) {
        setReservedHeight(next);
      } else {
        setReservedHeight(previous => Math.min(previous, next));
        // A native transform animation keeps the large image surface off the JS
        // frame loop. Read its current pose before cancelling an interrupted run.
        const animation = viewport.animate([{ transform: from }, { transform }], {
          duration: galleryTransition.duration * 1000, easing: `cubic-bezier(${galleryTransition.ease.join(",")})`,
        });
        movement.current = animation;
        animation.onfinish = () => {
          if (movement.current !== animation) return;
          movement.current = null;
          setReservedHeight(next);
        };
      }
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(node, { box: "border-box" });
    return () => observer.disconnect();
  }, [contentKey, reducedMotion]);
  useEffect(() => () => movement.current?.cancel(), []);
  return <GalleryMotion><div className="r-filter-layout">
    <div className="r-chips-region" inert={!chips.length} aria-hidden={!chips.length}>
      <div ref={content} className="r-chips" data-empty={!chips.length}>
        <AnimatePresence initial={false} mode="popLayout">
          {chips.map(chip => <FilterChip key={chip.key} chip={chip} reducedMotion={reducedMotion} />)}
          {!!chips.length && <motion.button key="clear" layout="position" type="button" className="r-text-action" onClick={clearFilters}
            initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ ...galleryTransition, duration: reducedMotion ? 0 : galleryTransition.duration }}>清除全部</motion.button>}
        </AnimatePresence>
      </div>
    </div>
    <div ref={viewportContent} className="r-filter-content" style={{ bottom: reservedHeight }}>
      {children}
    </div>
  </div></GalleryMotion>;
}

export function CanvasHeader({ filtersOpen, onFilters }: { filtersOpen: boolean; onFilters: () => void }) {
  const view = useWorkspace(state => state.viewMode);
  const cardSize = useWorkspace(state => state.galleryCardSize);
  const rowHeight = useWorkspace(state => state.tableRowHeight);
  const setSize = useWorkspace(state => state.setSize);
  const query = useRows(state => state.query);
  const total = useRows(state => state.total);
  const isTable = view === "table";
  if (view !== "gallery" && !isTable) return null;
  return <><header key={view} className="r-page-head"><h1 className="r-page-title">{viewLabel(view)}<span className="r-page-count">{formatCount(total)} 张符合条件</span></h1><div className="r-page-actions">
    <div className="r-size-control"><Grid2X2 size={11} aria-hidden="true" /><Slider aria-label={isTable ? "行高" : "卡片大小"} value={[isTable ? rowHeight : cardSize]} min={isTable ? 40 : 120} max={isTable ? 128 : 400} step={1} onValueChange={values => setSize(values[0])} /><Grid2X2 size={15} aria-hidden="true" /></div>
    <ImageSortMenu value={query.sort} onChange={sort => setQuery({ sort })} />
    <Button className="is-toggle" aria-expanded={filtersOpen} onClick={onFilters}><SlidersHorizontal size={15} />图片筛选</Button>
  </div></header></>;
}
