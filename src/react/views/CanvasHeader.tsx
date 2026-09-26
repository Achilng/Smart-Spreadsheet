import { Grid2X2, SlidersHorizontal, X } from "lucide-react";
import { clearFilters, setQuery, useRows } from "../state/library";
import { useWorkspace } from "../state/workspace";
import { viewLabel } from "../../lib/utils/view-modes";
import { formatCount } from "../../lib/utils/format";
import { Button, Menu, Slider } from "../ui/controls";
import { libraryFilterLabel } from "../../lib/utils/library-filters";
import { useGroups } from "../state/groups";
import { useDuplicates } from "../state/duplicates";

const sortOptions = [
  { value: "timeAsc", label: "时间正序", hint: "早期导入在前，新图片在后" },
  { value: "timeDesc", label: "时间倒序", hint: "新导入的图片优先显示" },
  { value: "recentlyUpdated", label: "最近更新", hint: "最近编辑或整理的图片在前" },
] as const;

export function CanvasHeader({ filtersOpen, onFilters }: { filtersOpen: boolean; onFilters: () => void }) {
  const view = useWorkspace(state => state.viewMode);
  const cardSize = useWorkspace(state => state.galleryCardSize);
  const rowHeight = useWorkspace(state => state.tableRowHeight);
  const setSize = useWorkspace(state => state.setSize);
  const query = useRows(state => state.query);
  const total = useRows(state => state.total);
  const isTable = view === "table";
  const groups = useGroups(state => state.list);
  const clusters = useDuplicates(state => state.clusters);
  const ordinary = view === "gallery" || isTable;
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
  return <>{view !== "group" && <div className="r-canvas-head"><h1>{viewLabel(view)}</h1><span className="r-count">{ordinary ? `${formatCount(total)} 张符合条件` : `${clusters.length} 组重复项`}</span><div className="r-canvas-controls">
    {ordinary && <div className="r-size-control"><Grid2X2 size={10} /><Slider aria-label={isTable ? "行高" : "卡片大小"} value={[isTable ? rowHeight : cardSize]} min={isTable ? 40 : 120} max={isTable ? 128 : 400} step={1} onValueChange={values => setSize(values[0])} /><Grid2X2 size={13} /></div>}
    {ordinary && <Menu className="r-sort-trigger" label={sortOptions.find(option => option.value === query.sort)?.label ?? "时间正序"} heading="选择图片顺序" items={sortOptions.map(option => ({ ...option, checked: option.value === query.sort, action: () => setQuery({ sort: option.value }) }))} />}
    <Button aria-expanded={filtersOpen} onClick={onFilters}><SlidersHorizontal size={15} />图片筛选</Button>
  </div></div>}
    {chips.length > 0 && <div className="r-chips">{chips.map(chip => <span className="r-filter-chip" key={chip.label}><span title={chip.label}>{chip.label}</span><Button variant="ghost" size="icon" aria-label={`移除筛选 ${chip.label}`} onClick={chip.remove}><X size={12} /></Button></span>)}<button type="button" className="r-text-action" onClick={clearFilters}>清除全部</button></div>}
  </>;
}
