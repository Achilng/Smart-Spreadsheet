import { useEffect, useState } from "react";
import { queryRows, type RowRecord, type SortMode } from "../../../lib/api/rows";
import type { DedupeMode, LibraryFilter, TagMatchMode } from "../../../lib/api/types";
import { defaultFilters, useLibrary } from "../../state/library";
import { errorText } from "../../../lib/utils/format";
import { Button, Checkbox, Input, Modal, Select } from "../../ui/controls";
import { Thumbnail } from "../../ui/Thumbnail";
import { FilterPanel } from "../FilterPanel";

export function MaterialGalleryPicker({ onClose, onChoose }: { onClose: () => void; onChoose: (id: number) => void }) {
  const [search, setSearch] = useState(""); const [tags, setTags] = useState<string[]>([]); const [page, setPage] = useState(0);
  const [filters, setFilters] = useState<LibraryFilter[]>([]); const [filterOpen, setFilterOpen] = useState(false);
  const [tagMode, setTagMode] = useState<TagMatchMode>("and"); const [dedupe, setDedupe] = useState<DedupeMode>("none"); const [sort, setSort] = useState<SortMode>("timeAsc");
  const [rows, setRows] = useState<RowRecord[]>([]); const [total, setTotal] = useState(0); const [selected, setSelected] = useState<number | null>(null);
  const [busy, setBusy] = useState(false); const [error, setError] = useState(""); const [retry, setRetry] = useState(0);
  const availableTags = useLibrary(state => state.tags);
  useEffect(() => {
    let active = true; setBusy(true); setError(""); setSelected(null);
    const timer = setTimeout(() => { void queryRows({ ...defaultFilters, search, tags, filters, tagMode, dedupe, sort, offset: page * 48, limit: 48 }).then(result => { if (active) { setRows(result.rows); setTotal(result.totalCount); } }).catch(cause => { if (active) setError(errorText(cause)); }).finally(() => { if (active) setBusy(false); }); }, 180);
    return () => { active = false; clearTimeout(timer); };
  }, [search, tags, page, retry, filters, tagMode, dedupe, sort]);
  return <><Modal open onClose={onClose} title="从画廊选择素材图片" width={1000} footer={<><Button onClick={onClose}>取消</Button><Button variant="primary" disabled={selected === null || busy} onClick={() => selected !== null && onChoose(selected)}>使用所选图片</Button></>}>
    <Input aria-label="搜索图库图片" placeholder="搜索图片、提示词…" value={search} onChange={event => { setSearch(event.target.value); setPage(0); }} />
    <div className="rm-picker-options"><Select label="选图 Tag 匹配方式" value={tagMode} onChange={value => { setTagMode(value); setPage(0); }} options={[["and", "同时匹配 Tag"], ["or", "任意匹配 Tag"]]} /><Select label="选图去重方式" value={dedupe} onChange={value => { setDedupe(value); setPage(0); }} options={[["none", "不去重"], ["positivePrompt", "正向提示词去重"], ["artists", "画师串去重"], ["vibes", "VIBE 去重"]]} /><Select label="选图排序" value={sort} onChange={value => { setSort(value); setPage(0); }} options={[["timeAsc", "最早导入"], ["timeDesc", "最新导入"], ["recentlyUpdated", "最近修改"]]} /><Button onClick={() => setFilterOpen(true)}>过滤{filters.length > 0 ? ` · ${filters.length}` : ""}</Button></div>
    <div className="rm-picker-tags">{availableTags.map(tag => <label key={tag.name}><Checkbox checked={tags.includes(tag.name)} onCheckedChange={() => { setTags(tags.includes(tag.name) ? tags.filter(name => name !== tag.name) : [...tags, tag.name]); setPage(0); }} />{tag.name}</label>)}</div>
    {error ? <p role="alert">{error}<Button onClick={() => setRetry(value => value + 1)}>重试</Button></p> : <div className="rm-picker-grid" aria-busy={busy}>{busy ? <p>正在读取图片…</p> : rows.length ? rows.map(row => <button key={row.id} className={selected === row.id ? "is-selected" : ""} aria-pressed={selected === row.id} onClick={() => setSelected(row.id)} onDoubleClick={() => onChoose(row.id)}><Thumbnail rowId={row.id} alt={`图片 ${row.id}`} /><span>{row.imagePath?.split(/[\\/]/).pop() ?? `图片 ${row.id}`}</span></button>) : <p>没有匹配的图片。</p>}</div>}
    <div className="rm-pagination"><Button disabled={!page || busy} onClick={() => setPage(value => value - 1)}>上一页</Button><span>{page + 1} / {Math.max(1, Math.ceil(total / 48))} · {total} 张</span><Button disabled={(page + 1) * 48 >= total || busy} onClick={() => setPage(value => value + 1)}>下一页</Button></div>
  </Modal>{filterOpen && <FilterPanel initialFilters={filters} onApply={value => { setFilters(value); setPage(0); }} onClose={() => setFilterOpen(false)} />}</>;
}
