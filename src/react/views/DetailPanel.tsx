import { ChevronsLeft, ChevronsRight } from "lucide-react";
import { useLibrary, useRows } from "../state/library";
import { useWorkspace } from "../state/workspace";
import { Button, Hint } from "../ui/controls";
import { DetailContents } from "./DetailContents";
import { requestDelete } from "../state/row-actions";
import { useTasks } from "../state/tasks";
import { rowFileName, rowResolution } from "../../lib/utils/row-display";

export function DetailPanel() {
  const row = useRows(state => state.activeRow);
  const open = useWorkspace(state => state.detailOpen);
  const setOpen = useWorkspace(state => state.setDetailOpen);
  const busy = useTasks(state => state.busy);
  const directory = useLibrary(state => state.snapshot?.dataDirectory);
  return <aside className="r-detail" data-open={open}>
    {!open ? <button type="button" className="r-detail-strip" aria-label="展开详情面板" onClick={() => setOpen(true)}><ChevronsLeft size={14} /></button> : <div className="r-detail-inner">
      <header className="r-detail-header"><div><h3 title={row ? rowFileName(row) ?? "详情" : "详情"}>{row ? rowFileName(row) || `第 ${row.sourceOrdinal} 行` : "详情"}</h3>{row && <small title={[rowResolution(row), row.time].filter(Boolean).join(" · ")}>{[rowResolution(row), row.time].filter(Boolean).join(" · ")}</small>}</div><div className="rd-header-actions">{row && <Button variant="danger" size="sm" disabled={busy} onClick={() => requestDelete({ kind: "explicit", rowIds: [row.id] }, 1)}>删除</Button>}<Hint text="收起详情面板"><Button variant="ghost" size="icon" aria-label="收起详情面板" onClick={() => setOpen(false)}><ChevronsRight size={15} /></Button></Hint></div></header>
      {!row ? <div className="r-detail-empty">点击图片或行查看详情</div> : <DetailContents key={`${directory}:${row.id}`} row={row} />}
    </div>}
  </aside>;
}
