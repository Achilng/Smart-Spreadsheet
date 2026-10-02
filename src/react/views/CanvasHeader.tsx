import { Grid2X2, SlidersHorizontal, X } from "lucide-react";
import { clearFilters, setQuery, useRows } from "../state/library";
import { useWorkspace } from "../state/workspace";
import { viewLabel } from "../../lib/utils/view-modes";
import { formatCount } from "../../lib/utils/format";
import { Button, Slider } from "../ui/controls";
import { ImageSortMenu } from "../ui/ImageSortMenu";
import { libraryFilterLabel } from "../../lib/utils/library-filters";
import { useGroups } from "../state/groups";

export function FilterChips() {
  const query = useRows(state => state.query);
  const groups = useGroups(state => state.list);
  const chips = [
    ...(query.search ? [{ label: `“${query.search}”`, remove: () => setQuery({ search: "" }) }] : []),
    ...query.tags.map(tag => ({ label: tag, remove: () => setQuery({ tags: query.tags.filter(value => value !== tag) }) })),
    ...(query.dedupe !== "none" ? [{ label: query.dedupe === "artists" ? "按画师串去重" : "按正向去重", remove: () => setQuery({ dedupe: "none" }) }] : []),
    ...(query.singleArtistOnly ? [{ label: "单画师", remove: () => setQuery({ singleArtistOnly: false }) }] : []),
    ...(query.artistFilter ? [{ label: `画师串：${query.artistFilter}`, remove: () => setQuery({ artistFilter: "" }) }] : []),
    ...(query.hasVibe ? [{ label: "包含 VIBE", remove: () => setQuery({ hasVibe: false }) }] : []),
    ...(query.untaggedOnly ? [{ label: "无 Tag", remove: () => setQuery({ untaggedOnly: false }) }] : []),
    ...(query.hideGrouped ? [{ label: "隐藏已分组", remove: () => setQuery({ hideGrouped: false }) }] : []),
    ...query.filters.map((filter, index) => ({ label: libraryFilterLabel(filter, groups), remove: () => setQuery({ filters: query.filters.filter((_, cursor) => cursor !== index) }) })),
  ];
  if (!chips.length) return null;
  return <div className="r-chips">{chips.map(chip => <span className="r-filter-chip" key={chip.label}><span title={chip.label}>{chip.label}</span><Button variant="ghost" size="icon" aria-label={`移除筛选 ${chip.label}`} onClick={chip.remove}><X size={12} /></Button></span>)}<button type="button" className="r-text-action" onClick={clearFilters}>清除全部</button></div>;
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
  return <><header className="r-page-head"><h1 className="r-page-title">{viewLabel(view)}<span className="r-page-count">{formatCount(total)} 张符合条件</span></h1><div className="r-page-actions">
    <div className="r-size-control"><Grid2X2 size={11} aria-hidden="true" /><Slider aria-label={isTable ? "行高" : "卡片大小"} value={[isTable ? rowHeight : cardSize]} min={isTable ? 40 : 120} max={isTable ? 128 : 400} step={1} onValueChange={values => setSize(values[0])} /><Grid2X2 size={15} aria-hidden="true" /></div>
    <ImageSortMenu value={query.sort} onChange={sort => setQuery({ sort })} />
    <Button className="is-toggle" aria-expanded={filtersOpen} onClick={onFilters}><SlidersHorizontal size={15} />图片筛选</Button>
  </div></header><FilterChips /></>;
}
