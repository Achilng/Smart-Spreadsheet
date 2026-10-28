import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { motion, MotionConfig } from "motion/react";
import { useReducedMotionPreference } from "../../ui/use-reduced-motion";
import { useGridColumns } from "../../ui/use-grid-columns";
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
import { Button, Checkbox, Menu, RightClickMenu, type MenuItem } from "../../ui/controls";
import { Thumbnail } from "../../ui/Thumbnail";
import { RowContextMenu } from "../../ui/RowContextMenu";
import { useNavigation } from "../../state/navigation";
import { VirtualRows } from "../../ui/VirtualRows";

export function SectionHeader({ label, count, expanded, onToggle, items, suffix }: { label: string; count?: number; expanded: boolean; onToggle: () => void; items?: MenuItem[]; suffix?: string }) {
  const content = <div className="r-section-header"><button type="button" aria-expanded={expanded} onClick={onToggle}><ChevronRight size={14} className={expanded ? "is-expanded" : ""} /><span title={label}>{label}</span>{suffix && <small title={suffix}>{suffix}</small>}{count !== undefined && <em>{count.toLocaleString()} 张</em>}</button>{items && <Menu label={<><MoreHorizontal size={15} /><span className="sr-only">{label}操作</span></>} items={items} />}</div>;
  return <RightClickMenu items={items}>{content}</RightClickMenu>;
}
export function SectionList({ positionKey, loading, version, children }: { positionKey: string; loading: boolean; version: number; children: ReactNode }) {
  const reducedMotion = useReducedMotionPreference();
  const ref = useRef<HTMLDivElement>(null), restoring = useRef(true);
  const reset = useRows(state => state.resetToken);
  const navigationRestoring = useNavigation(state => state.restoring);
  useLayoutEffect(() => {
    restoring.current = true;
    if (loading || navigationRestoring || !ref.current) return;
    return restoreScrollPosition(ref.current, positionKey, 60, undefined, () => { restoring.current = false; });
  }, [positionKey, reset, loading, version, navigationRestoring]);
  return <MotionConfig reducedMotion="never" transition={{ layout: { duration: reducedMotion ? 0 : .28, ease: [.22, 1, .36, 1] } }}><motion.div layoutScroll className="r-section-list" ref={ref} tabIndex={0} onScroll={event => { if (!restoring.current && !navigationRestoring && !loading) saveScrollPosition(positionKey, event.currentTarget.scrollTop); }}>{children}</motion.div></MotionConfig>;
}
function MembersFooter({ data, onLoad }: { data?: SectionMembers; onLoad: (more?: boolean) => void }) {
  return <div className="r-section-footer">
    {(!data || (data.loading && !data.rows.length)) && <p className="r-group-status" role="status">正在加载…</p>}
    {data?.error && <div className="r-group-status" role="alert"><p className="r-group-error">加载失败：{data.error}</p><Button onClick={() => onLoad(Boolean(data.rows.length))}>重试</Button></div>}
    {data && !data.loading && !data.error && !data.rows.length && <p className="r-group-status">没有符合条件的图片。</p>}
    {data && !data.error && !data.exhausted && data.rows.length < data.totalCount && <Button className="r-section-load" disabled={data.loading} onClick={() => onLoad(true)}>{data.loading ? "加载中…" : `加载更多（还有 ${(data.totalCount - data.rows.length).toLocaleString()} 张）`}</Button>}
  </div>;
}

export interface BrowseSection {
  key: string; label?: string; count?: number; suffix?: string; expanded: boolean;
  data?: SectionMembers; items?: MenuItem[]; onToggle?: () => void; onLoad: (more?: boolean) => void;
}
type BrowseBlock = { section: BrowseSection; kind: "header" | "images" | "footer"; start: number };

/** Flatten sections before windowing so an expanded album never mounts all its images. */
export function SectionBrowseList({ sections, scope, order }: { sections: BrowseSection[]; scope: "groups" | "duplicates"; order: number[] }) {
  const { ref, columns: measured } = useGridColumns(120, 12);
  const columns = measured || 1;
  const blocks = useMemo(() => {
    const result: BrowseBlock[] = [];
    for (const section of sections) {
      if (section.label !== undefined) result.push({ section, kind: "header", start: 0 });
      if (!section.expanded) continue;
      for (let start = 0; start < (section.data?.rows.length ?? 0); start += columns) result.push({ section, kind: "images", start });
      result.push({ section, kind: "footer", start: 0 });
    }
    return result;
  }, [sections, columns]);
  const stickyRows = useMemo(() => blocks.flatMap((block, index) => block.kind === "header" ? [index] : []), [blocks]);
  return <div ref={ref} className="r-virtual-sections">
    <VirtualRows count={blocks.length} layoutKey={columns} stickyRows={stickyRows} itemKey={index => {
      const block = blocks[index]; return `${block.section.key}:${block.kind}:${block.start}`;
    }} estimateSize={index => blocks[index].kind === "images" ? 179 : blocks[index].kind === "header" ? 48 : 60} render={index => {
      const { section, kind, start } = blocks[index];
      if (kind === "header") return <SectionHeader label={section.label!} count={section.count} suffix={section.suffix} expanded={section.expanded} onToggle={section.onToggle!} items={section.items} />;
      if (kind === "footer") return <MembersFooter data={section.data} onLoad={section.onLoad} />;
      return <div className="r-section-grid r-virtual-image-row" role="list" style={{ gridTemplateColumns: `repeat(${columns}, 120px)` }}>
        {section.data!.rows.slice(start, start + columns).map(row => <GroupSectionCard key={row.id} row={row} order={order} scope={scope} />)}
      </div>;
    }} />
  </div>;
}

export function SectionMembersGrid({ data, scope, order, onLoad }: { data?: SectionMembers; scope: "groups" | "duplicates"; order: number[]; onLoad: (more?: boolean) => void }) {
  const sections = useMemo(() => [{ key: "album", expanded: true, data, onLoad }], [data, onLoad]);
  return <SectionBrowseList sections={sections} scope={scope} order={order} />;
}
export const GroupSectionCard = memo(function GroupSectionCard({ row, order, scope }: { row: RowRecord; order: number[]; scope: "groups" | "duplicates" }) {
  const selected = useSelection(state => isSelected(row.id, state)), hasSelection = useSelection(state => selectedCount(state) > 0);
  const active = useRows(state => state.activeRow?.id === row.id), dragged = useRef(false);
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
  return <RowContextMenu row={row}><div role="listitem" onContextMenu={() => useRows.setState({ activeRow: row })} className={`r-section-card${active ? " is-active" : ""}${selected ? " is-checked" : ""}${hasSelection ? " has-selection" : ""}`} title={[label, resolution, row.imagePath].filter(Boolean).join("\n")}>
    <Checkbox className="r-section-check" aria-label={`选择第 ${row.sourceOrdinal} 行`} checked={selected} onClick={event => { event.stopPropagation(); toggleOrderedRow(row.id, order, scope, event.shiftKey); }} />
    <button className="r-section-card-main" type="button" aria-label={`查看第 ${row.sourceOrdinal} 行详情`} onMouseDown={event => {
      dragged.current = false;
      if (hasImage && event.target instanceof Element && event.target.closest(".r-section-thumb")) beginFileDrag(event.nativeEvent, row.id, () => { dragged.current = true; });
    }} onClick={event => {
      if (dragged.current) { dragged.current = false; return; }
      if (event.ctrlKey || event.metaKey || event.shiftKey) toggleOrderedRow(row.id, order, scope, event.shiftKey);
      else useRows.setState({ activeRow: row });
    }}>
      <div className="r-section-thumb">{hasImage ? <Thumbnail rowId={row.id} alt={label} /> : <span className="r-section-no-image"><ImageOff size={22} /><small>无图片</small></span>}{(badge || Boolean(vibeRefs)) && <span className="r-section-badges">{badge && <span className={`version-badge r-section-model ${badge.className}`} title={`作画模型：${row.generationModel}`}>{badge.label}</span>}{Boolean(vibeRefs) && <span className="vibe-badge">VIBE ×{vibeRefs}</span>}</span>}</div>
      <span className="r-section-label">{row.artistRepresentative && <span className="r-representative-label">代表图 · </span>}{label}</span>{resolution && <span className="r-section-resolution">{resolution}</span>}
    </button>
  </div></RowContextMenu>;
});
