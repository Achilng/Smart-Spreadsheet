import { ListFilter } from "lucide-react";
import { useState } from "react";
import { ContextMenu, RadioGroup } from "radix-ui";
import { hasActiveFilters, refreshTags, setQuery, useLibrary, useRows } from "../state/library";
import { useWorkspace } from "../state/workspace";
import { TagManagementDialog } from "./TagManagementDialog";
import { Button, Checkbox, Hint } from "../ui/controls";
import { formatCount } from "../../lib/utils/format";
import { tagColorFor } from "../../lib/utils/tag-colors";

export function TagSidebar({ onFilter }: { onFilter: () => void }) {
  const tags = useLibrary(state => state.tags);
  const tagError = useLibrary(state => state.tagError);
  const query = useRows(state => state.query);
  const view = useWorkspace(state => state.viewMode);
  const [editing, setEditing] = useState<{ name: string; mode: "rename" | "delete" } | null>(null);
  const filtered = hasActiveFilters(query);
  const entries = [...tags, ...query.tags.filter(name => !tags.some(tag => tag.name === name)).map(name => ({ name, rowCount: 0 }))];
  return <aside className="r-sidebar"><header className="r-sidebar-header"><h3>筛选</h3><p>{filtered ? "已启用筛选" : "未启用筛选"}</p></header>
    <section className="r-filter-group" aria-label="去重与筛选"><h4>显示</h4>
      <label className="r-check-row"><Checkbox checked={query.filters.some(filter => filter.type === "favorite")} onCheckedChange={checked => setQuery({ filters: [...query.filters.filter(filter => filter.type !== "favorite"), ...(checked ? [{ type: "favorite" } as const] : [])] })} /><span>仅显示收藏</span></label>
      {([ ["positivePrompt", "按正向提示词去重"], ["artists", "按画师串去重"] ] as const).map(([mode, label]) => <label key={mode} className="r-check-row">
        <Checkbox disabled={view === "group"} checked={query.dedupe === mode} onCheckedChange={checked => setQuery({ dedupe: checked ? mode : "none" })} /><span>{label}</span>
      </label>)}
      <Button className="r-filter-launch" onClick={onFilter}><ListFilter size={15} /><strong>过滤</strong><small>选择条件</small></Button>
    </section>
    <div className="r-tag-heading"><h4>Tag</h4><RadioGroup.Root className="r-tag-mode" aria-label="Tag 匹配方式" orientation="horizontal" value={query.tagMode} onValueChange={mode => { if (mode === "and" || mode === "or") setQuery({ tagMode: mode }); }}>
      <Hint text="同时包含全部所选 Tag"><RadioGroup.Item className="r-tag-mode-option" value="and" aria-label="AND：同时包含全部所选 Tag">AND</RadioGroup.Item></Hint>
      <Hint text="包含任意一个所选 Tag"><RadioGroup.Item className="r-tag-mode-option" value="or" aria-label="OR：包含任意一个所选 Tag">OR</RadioGroup.Item></Hint>
    </RadioGroup.Root></div>
    <div className="r-tag-list">{tagError ? <div className="r-list-note"><p>Tag 列表加载失败：{tagError}</p><Button onClick={() => void refreshTags()}>重试</Button></div> : entries.length === 0 ? <p className="r-list-note">还没有 Tag。选中图片后点“编辑 Tag”即可创建。</p> : entries.map(tag => <ContextMenu.Root key={tag.name}><ContextMenu.Trigger asChild><label className="r-check-row r-tag-row" data-active={query.tags.includes(tag.name)}>
      <Checkbox checked={query.tags.includes(tag.name)} onCheckedChange={checked => setQuery({ tags: checked ? [...query.tags, tag.name] : query.tags.filter(value => value !== tag.name) })} />
      <span className="r-tag-swatch" style={{ background: tagColorFor(tag.name, tags).background }} /><span className="r-tag-name" title={tag.name}>{tag.name}</span><small>{formatCount(tag.rowCount)}</small>
    </label></ContextMenu.Trigger><ContextMenu.Portal><ContextMenu.Content className="r-menu"><ContextMenu.Item className="r-menu-item" onSelect={() => setEditing({ name: tag.name, mode: "rename" })}>重命名</ContextMenu.Item><ContextMenu.Item className="r-menu-item is-danger" onSelect={() => setEditing({ name: tag.name, mode: "delete" })}>删除 Tag</ContextMenu.Item></ContextMenu.Content></ContextMenu.Portal></ContextMenu.Root>)}</div>
    {editing && <TagManagementDialog key={editing.name + editing.mode} {...editing} onClose={() => setEditing(null)} />}
  </aside>;
}
