import { useEffect, useLayoutEffect, useRef, type CSSProperties } from "react";
import { ensurePage, PAGE_SIZE, reloadRows, clearFilters, useRows } from "../state/library";
import { isSelected, toggleRow, useSelection } from "../state/selection";
import { useWorkspace } from "../state/workspace";
import { useViewport } from "../ui/use-viewport";
import { Button, Checkbox } from "../ui/controls";
import { Thumbnail } from "../ui/Thumbnail";
import { LlmBadge } from "../ui/LlmBadge";
import { thumbnails } from "../ui/use-image";
import { rememberVisibleRange } from "../../lib/stores/view-state";

import { RowContextMenu } from "../ui/RowContextMenu";
import { beginFileDrag } from "../state/file-drag";

export function Table() {
  const dragged = useRef(false);
  const { viewport, size, onScroll } = useViewport("table");
  const height = useWorkspace(state => state.tableRowHeight);
  const pages = useRows(state => state.pages);
  const total = useRows(state => state.total);
  const loading = useRows(state => state.loading || state.refreshing);
  const error = useRows(state => state.error);
  const activeId = useRows(state => state.activeRow?.id);
  const selection = useSelection();
  const first = Math.max(0, Math.floor((size.top - 36) / height) - 6);
  const last = Math.min(total, first + Math.ceil(size.height / height) + 12);
  const indices = Array.from({ length: Math.max(0, last - first) }, (_, index) => first + index);
  const pageKey = [...new Set(indices.map(index => Math.floor(index / PAGE_SIZE)))].join(",");
  const thumbWidth = Math.max(52, Math.round(height * 1.15));
  const columns = `36px ${thumbWidth}px 64px 150px minmax(0,1.8fr) minmax(0,1.8fr) minmax(0,1.1fr) minmax(0,1.3fr)`;
  useEffect(() => {
    if (!loading && pageKey) for (const page of pageKey.split(",").map(Number)) void ensurePage(page);
  }, [pageKey, loading]);
  useLayoutEffect(() => { if (!loading && last > first) rememberVisibleRange("table", first, last - 1); }, [first, last, loading]);
  const visibleIds = indices.map(index => pages.get(Math.floor(index / PAGE_SIZE))?.[index % PAGE_SIZE]?.id).filter((id): id is number => id !== undefined).join(",");
  useEffect(() => { thumbnails.retain(new Set(visibleIds ? visibleIds.split(",").map(Number) : [])); }, [visibleIds]);
  return <div className="r-table" ref={viewport} onScroll={onScroll} role="table" aria-label="图片表格" aria-rowcount={total + 1} aria-colcount={8} tabIndex={0} style={{ "--table-columns": columns } as CSSProperties}>
    {error ? <div className="r-state-message"><p>加载失败：{error}</p><Button onClick={() => void reloadRows()}>重试</Button></div> : total === 0 ? <div className="r-state-message"><p>{loading ? "正在加载…" : "没有符合条件的图片"}</p>{!loading && <Button onClick={clearFilters}>清除全部筛选</Button>}</div> : <>
      <div className="r-table-head" role="row">{["", "图片", "行号", "时间", "正向提示词", "角色提示词", "画师串", "Tags"].map((label, index) => <span key={index} role="columnheader">{label}</span>)}</div>
      <div className="r-table-spacer" role="rowgroup" style={{ height: total * height }}>{indices.map(index => {
        const row = pages.get(Math.floor(index / PAGE_SIZE))?.[index % PAGE_SIZE];
        if (!row) return <div className="r-table-row r-image-placeholder" key={`placeholder-${index}`} style={{ top: index * height, height }} />;
        const show = () => useRows.setState({ activeRow: row });
        return <RowContextMenu key={row.id} row={row}><div onContextMenu={() => useRows.setState({ activeRow: row })} role="row" tabIndex={0} aria-rowindex={index + 2} aria-selected={isSelected(row.id, selection)} data-active={row.id === activeId} className="r-table-row" style={{ top: index * height, height }}
          onClick={event => { if (dragged.current) { dragged.current = false; return; } if (event.ctrlKey || event.metaKey || (event.shiftKey && selection.anchor !== null)) toggleRow(row.id, index, event.shiftKey); else show(); }}
          onKeyDown={event => { if (event.target === event.currentTarget && (event.key === "Enter" || event.key === " ")) { event.preventDefault(); show(); } }}>
          <div role="cell" className="r-table-check"><Checkbox aria-label={`选择第 ${row.sourceOrdinal} 行`} checked={isSelected(row.id, selection)} onClick={event => { event.stopPropagation(); toggleRow(row.id, index, event.shiftKey); }} /></div>
          <div role="cell" onMouseDown={event => { dragged.current = false; if (row.imagePath || row.storedImagePath) beginFileDrag(event.nativeEvent, row.id, () => { dragged.current = true; }); }} className="r-table-thumb" style={{ height: Math.max(24, height - 16), width: thumbWidth - 12 }}><Thumbnail hasImage={Boolean(row.imagePath || row.storedImagePath)} rowId={row.id} alt={`第 ${row.sourceOrdinal} 行缩略图`} /></div>
          <div role="cell" className="r-muted">#{row.sourceOrdinal}</div>
          <div role="cell" title={row.time ?? ""}>{row.time ?? "—"}</div>
          <div role="cell" title={row.positivePrompt ?? ""}>{row.positivePrompt ?? "—"}</div>
          <div role="cell" title={row.characterPrompt ?? ""}>{row.characterPrompt ?? "—"}</div>
          <div role="cell" title={row.artists ?? ""}><LlmBadge source={row.artistLlm} />{row.artists || (row.artistLlm ? "未识别到画风" : "—")}</div>
          <div role="cell" className="r-table-tags" title={row.tags.join(", ")}>{row.tags.slice(0, 3).map(tag => <span key={tag}>{tag}</span>)}{row.tags.length > 3 && <span>+{row.tags.length - 3}</span>}{!row.tags.length && "—"}</div>
        </div></RowContextMenu>;
      })}</div>
    </>}
  </div>;
}
