import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { ContextMenu } from "radix-ui";
import { ChevronRight, ImageOff, MoreHorizontal } from "lucide-react";
import type { RowRecord } from "../../../lib/api";
import { modelVersionBadge } from "../../../lib/utils/model-version";
import { rowFileName, rowResolution } from "../../../lib/utils/row-display";
import { vibeStatuses } from "../../../lib/images/vibe-statuses";
import { restoreScrollPosition, saveScrollPosition } from "../../../lib/stores/view-state";
import { useRows } from "../../state/library";
import { isSelected, selectedCount, toggleOrderedRow, useSelection } from "../../state/selection";
import type { SectionMembers } from "../../state/groups";
import { beginFileDrag } from "../../state/file-drag";
import { Button, Checkbox, Menu, type MenuItem } from "../../ui/controls";
import { Thumbnail } from "../../ui/Thumbnail";
import { RowContextMenu } from "../../ui/RowContextMenu";
import { useNavigation } from "../../state/navigation";

export function SectionHeader({ label, count, expanded, onToggle, items, suffix }: { label: string; count?: number; expanded: boolean; onToggle: () => void; items?: MenuItem[]; suffix?: string }) {
  const content = <div className="r-section-header"><button type="button" aria-expanded={expanded} onClick={onToggle}><ChevronRight size={14} className={expanded ? "is-expanded" : ""} /><span title={label}>{label}</span>{suffix && <small title={suffix}>{suffix}</small>}{count !== undefined && <em>{count.toLocaleString()} 张</em>}</button>{items && <Menu label={<><MoreHorizontal size={15} /><span className="sr-only">{label}操作</span></>} items={items} />}</div>;
  if (!items) return content;
  return <ContextMenu.Root><ContextMenu.Trigger asChild>{content}</ContextMenu.Trigger><ContextMenu.Portal><ContextMenu.Content className="r-menu" collisionPadding={8}>{items.map(item => <ContextMenu.Item key={item.label} className={`r-menu-item${item.danger ? " is-danger" : ""}`} disabled={item.disabled} onSelect={item.action}>{item.label}</ContextMenu.Item>)}</ContextMenu.Content></ContextMenu.Portal></ContextMenu.Root>;
}
export function SectionList({ scope, loading, version, children }: { scope: "groups" | "duplicates"; loading: boolean; version: number; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null), restoring = useRef(false);
  const reset = useRows(state => state.resetToken);
  const navigationRestoring = useNavigation(state => state.restoring);
  useLayoutEffect(() => {
    if (loading || navigationRestoring || !ref.current) return;
    restoring.current = true;
    restoreScrollPosition(ref.current, scope, 60, undefined, () => { restoring.current = false; });
  }, [scope, reset, loading, version, navigationRestoring]);
  return <div className="r-section-list" ref={ref} tabIndex={0} onScroll={event => { if (!restoring.current && !navigationRestoring && !loading) saveScrollPosition(scope, event.currentTarget.scrollTop); }}>{children}</div>;
}
export function SectionMembersGrid({ data, scope, order, limit, onReveal, onLoad }: { data?: SectionMembers; scope: "groups" | "duplicates"; order: number[]; limit: number; onReveal: () => void; onLoad: (more?: boolean) => void }) {
  return <div className="r-section-grid" role="list">
    {(!data || (data.loading && !data.rows.length)) && <p className="r-group-status" role="status">正在加载…</p>}
    {data?.rows.slice(0, limit).map(row => <GroupSectionCard key={row.id} row={row} order={order} scope={scope} />)}
    {data?.error && <div className="r-group-status" role="alert"><p className="r-group-error">加载失败：{data.error}</p><Button onClick={() => onLoad(Boolean(data.rows.length))}>重试</Button></div>}
    {data && !data.loading && !data.error && !data.rows.length && <p className="r-group-status">没有符合条件的图片。</p>}
    {data && limit < data.rows.length && <RevealMore onReveal={onReveal} />}
    {data && !data.error && limit >= data.rows.length && data.rows.length < data.totalCount && <Button className="r-section-load" disabled={data.loading} onClick={() => onLoad(true)}>{data.loading ? "加载中…" : `加载更多（还有 ${(data.totalCount - data.rows.length).toLocaleString()} 张）`}</Button>}
  </div>;
}
function RevealMore({ onReveal }: { onReveal: () => void }) {
  const ref = useRef<HTMLDivElement>(null), callback = useRef(onReveal); callback.current = onReveal;
  useEffect(() => {
    if (!ref.current) return;
    const observer = new IntersectionObserver(entries => { if (entries.some(entry => entry.isIntersecting)) callback.current(); }, { rootMargin: "200px" });
    observer.observe(ref.current); return () => observer.disconnect();
  }, [onReveal]);
  return <div className="r-section-sentinel" ref={ref} />;
}
export function GroupSectionCard({ row, order, scope }: { row: RowRecord; order: number[]; scope: "groups" | "duplicates" }) {
  const selection = useSelection(), active = useRows(state => state.activeRow?.id === row.id);
  const selected = isSelected(row.id, selection), dragged = useRef(false);
  const hasImage = Boolean(row.imagePath?.trim() || row.storedImagePath?.trim());
  const [vibeRefs, setVibeRefs] = useState<number | null>(row.vibeReferenceCount);
  useEffect(() => {
    let disposed = false;
    setVibeRefs(row.vibeReferenceCount);
    if (row.vibeReferenceCount === null && (row.imagePath?.trim() || row.storedImagePath?.trim())) {
      void vibeStatuses.load(row.id).then(count => { if (!disposed) setVibeRefs(count); }, () => {});
    }
    return () => { disposed = true; };
  }, [row.id, row.vibeReferenceCount, row.imagePath, row.storedImagePath]);
  const label = rowFileName(row) ?? row.artists?.split("\n")[0]?.trim() ?? `#${row.sourceOrdinal}`;
  const resolution = rowResolution(row), badge = modelVersionBadge(row.generationModel);
  return <RowContextMenu row={row}><div role="listitem" onContextMenu={() => useRows.setState({ activeRow: row })} className={`r-section-card${active ? " is-active" : ""}${selected ? " is-checked" : ""}${selectedCount(selection) ? " has-selection" : ""}`} title={[label, resolution, row.imagePath].filter(Boolean).join("\n")}>
    <Checkbox className="r-section-check" aria-label={`选择第 ${row.sourceOrdinal} 行`} checked={selected} onClick={event => { event.stopPropagation(); toggleOrderedRow(row.id, order, scope, event.shiftKey); }} />
    <button className="r-section-card-main" type="button" aria-label={`查看第 ${row.sourceOrdinal} 行详情`} onMouseDown={event => {
      dragged.current = false;
      if (hasImage && event.target instanceof Element && event.target.closest(".r-section-thumb")) beginFileDrag(event.nativeEvent, row.id, () => { dragged.current = true; });
    }} onClick={event => {
      if (dragged.current) { dragged.current = false; return; }
      if (event.ctrlKey || event.metaKey || event.shiftKey) toggleOrderedRow(row.id, order, scope, event.shiftKey);
      else useRows.setState({ activeRow: row });
    }}>
      <div className="r-section-thumb">{hasImage ? <Thumbnail rowId={row.id} alt={label} /> : <span className="r-section-no-image"><ImageOff size={22} /><small>无图片</small></span>}{badge && <span className={`version-badge r-section-model ${badge.className}`} title={`作画模型：${row.generationModel}`}>{badge.label}</span>}{Boolean(vibeRefs) && <span className="vibe-badge r-section-vibe">VIBE ×{vibeRefs}</span>}{row.tags.length > 0 && <span className="r-section-tags">{row.tags.slice(0, 2).join(" · ")}{row.tags.length > 2 ? ` +${row.tags.length - 2}` : ""}</span>}</div>
      <span className="r-section-label">{label}</span>{resolution && <span className="r-section-resolution">{resolution}</span>}
    </button>
  </div></RowContextMenu>;
}
