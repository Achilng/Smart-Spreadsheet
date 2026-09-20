import { ListFilter } from "lucide-react";
import { setQuery, useLibrary, useRows } from "../state/library";
import { Button, Checkbox } from "../ui/controls";
import { formatCount } from "../../lib/utils/format";
import { tagColorFor } from "../../lib/utils/tag-colors";

export function TagSidebar({ onFilter }: { onFilter: () => void }) {
  const tags = useLibrary(state => state.tags);
  const tagError = useLibrary(state => state.tagError);
  const query = useRows(state => state.query);
  const filtered = Boolean(query.tags.length || query.search || query.dedupe !== "none" || query.filters.length);
  return <aside className="r-sidebar"><header className="r-sidebar-header"><h3>筛选</h3><p>{filtered ? "已启用筛选" : "未启用筛选"}</p></header>
    <section className="r-filter-group" aria-label="去重与筛选"><h4>显示</h4>
      {([ ["positivePrompt", "按正向提示词去重"], ["artists", "按画师串去重"] ] as const).map(([mode, label]) => <label key={mode} className="r-check-row">
        <Checkbox checked={query.dedupe === mode} onCheckedChange={checked => setQuery({ dedupe: checked ? mode : "none" })} /><span>{label}</span>
      </label>)}
      <Button className="r-filter-launch" onClick={onFilter}><ListFilter size={15} /><strong>过滤</strong><small>选择条件</small></Button>
    </section>
    <div className="r-tag-heading"><h4>Tag</h4><button type="button" onClick={() => setQuery({ tagMode: query.tagMode === "and" ? "or" : "and" })} title="切换 Tag 筛选的组合方式">{query.tagMode.toUpperCase()} 模式 ⌄</button></div>
    <div className="r-tag-list">{tagError ? <p className="r-list-note">Tag 列表加载失败：{tagError}</p> : tags.length === 0 ? <p className="r-list-note">还没有 Tag。选中图片后点“编辑 Tag”即可创建。</p> : tags.map(tag => <label key={tag.name} className="r-check-row r-tag-row" data-active={query.tags.includes(tag.name)}>
      <Checkbox checked={query.tags.includes(tag.name)} onCheckedChange={checked => setQuery({ tags: checked ? [...query.tags, tag.name] : query.tags.filter(value => value !== tag.name) })} />
      <span className="r-tag-swatch" style={{ background: tagColorFor(tag.name, tags).background }} /><span className="r-tag-name" title={tag.name}>{tag.name}</span><small>{formatCount(tag.rowCount)}</small>
    </label>)}</div>
  </aside>;
}
