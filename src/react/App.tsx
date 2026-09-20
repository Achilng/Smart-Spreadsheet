import { useEffect, useState } from "react";
import { initializeLibrary, useLibrary, useRows } from "./state/library";
import { Button, Checkbox, Input, Modal, Textarea, Tooltip } from "./ui/controls";
import { TopBar } from "./views/TopBar";
import { TagSidebar } from "./views/TagSidebar";
import { CanvasHeader } from "./views/CanvasHeader";
import { Gallery } from "./views/Gallery";
import { DetailPanel } from "./views/DetailPanel";
import { Notices } from "./ui/Notices";
import { clearSelection, selectedCount, useSelection } from "./state/selection";
import { WindowControls } from "./ui/WindowControls";

/** First migration slice: the real query/image pipeline and same-layout UI shell.
 * Remaining workflows are connected in subsequent slices before this becomes the default entry. */
export function App() {
  const loaded = useLibrary(state => state.loaded);
  const error = useLibrary(state => state.error ?? state.snapshot?.startupError);
  const configured = useLibrary(state => Boolean(state.snapshot?.dataDirectory));
  const refreshing = useRows(state => state.refreshing);
  const count = useSelection(selectedCount);
  const [dialog, setDialog] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  useEffect(() => { void initializeLibrary(); }, []);
  return <Tooltip.Provider delayDuration={550} skipDelayDuration={150}>
    {!loaded || error || !configured ? <div className="r-flow"><header className="r-flow-header"><strong>智能表格</strong><WindowControls /></header><div className="r-state-message"><p>{error || (loaded ? "请先配置数据目录" : "正在读取应用状态…")}</p>{error && <Button onClick={() => void initializeLibrary()}>重试</Button>}</div></div>
      : <div className="r-workspace"><TopBar onImport={() => setDialog("导入图片")} onExport={() => setDialog("导出选项")} onToolbox={() => setDialog("工具箱")} />
        <div className="r-workspace-body"><TagSidebar onFilter={() => setDialog("过滤")} /><main className="r-main-area">
          {refreshing && <div className="r-refresh-bar" role="status" aria-label="正在刷新" />}<CanvasHeader /><Gallery />
          {count > 0 && <div className="r-selection-bar"><strong>已选择 {count} 张</strong><div><Button variant="ghost" onClick={clearSelection}>取消选择</Button><Button onClick={() => setDialog("编辑 Tag")}>编辑 Tag</Button><Button variant="primary" onClick={() => setDialog("导出选项")}>导出</Button></div></div>}
        </main><DetailPanel /></div>
      </div>}
    <Modal open={dialog !== null} onClose={() => setDialog(null)} title={dialog ?? ""} description="组件交互预览，尚未连接此操作。" footer={<Button onClick={() => setDialog(null)}>关闭</Button>}>
      <label className="r-field"><span>名称</span><Input value={draft} onChange={event => setDraft(event.target.value)} placeholder="输入内容…" /></label>
      <label className="r-field"><span>备注</span><Textarea placeholder="补充说明…" rows={4} /></label>
      <label className="r-check-row"><Checkbox /><span>包含当前选中的图片</span></label>
    </Modal><Notices />
  </Tooltip.Provider>;
}
