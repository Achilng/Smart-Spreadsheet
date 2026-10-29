import { Grid2X2, SlidersHorizontal, X } from "lucide-react";
import { forwardRef, useLayoutEffect, useRef, useState } from "react";
import { AnimatePresence, motion, useIsPresent, useMotionValue } from "motion/react";
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
    initial={{ opacity: 0, y: reducedMotion ? 0 : -4 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: reducedMotion ? 0 : -4 }}
    transition={{ ...galleryTransition, duration: reducedMotion ? 0 : galleryTransition.duration }}>
    <span title={chip.label}>{chip.label}</span><Button variant="ghost" size="icon" aria-label={`移除筛选 ${chip.label}`} onClick={chip.remove}><X size={12} /></Button>
  </motion.span>;
});

export function FilterChips() {
  const query = useRows(state => state.query);
  const groups = useGroups(state => state.list);
  const reducedMotion = useReducedMotionPreference();
  const content = useRef<HTMLDivElement>(null);
  const height = useMotionValue(0);
  const [naturalHeight, setNaturalHeight] = useState(0);
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
    const node = content.current;
    if (!node) return;
    const measure = () => {
      const next = node.offsetHeight;
      // Measure the natural content, not the animated shell. This also catches
      // wrapping when side panels change width, without a ResizeObserver loop.
      if (targetHeight.current === null || reducedMotion) height.jump(next);
      setNaturalHeight(next);
      targetHeight.current = next;
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(node, { box: "border-box" });
    return () => observer.disconnect();
  }, [contentKey, height, reducedMotion]);
  return <GalleryMotion><motion.div className="r-chips-region" style={{ height }} initial={false} animate={{ height: naturalHeight }}
    transition={{ ...galleryTransition, duration: reducedMotion ? 0 : galleryTransition.duration }} inert={!chips.length} aria-hidden={!chips.length}>
    <div ref={content} className="r-chips" data-empty={!chips.length}>
      <AnimatePresence initial={false} mode="popLayout">
        {chips.map(chip => <FilterChip key={chip.key} chip={chip} reducedMotion={reducedMotion} />)}
        {!!chips.length && <motion.button key="clear" layout="position" type="button" className="r-text-action" onClick={clearFilters}
          initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ ...galleryTransition, duration: reducedMotion ? 0 : galleryTransition.duration }}>清除全部</motion.button>}
      </AnimatePresence>
    </div>
  </motion.div></GalleryMotion>;
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
  </div></header><FilterChips /></>;
}
