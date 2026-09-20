import { FolderOpen, RefreshCw } from "lucide-react";
import { openRejectedImagesDirectory } from "../../../lib/api";
import { useLibrary } from "../../state/library";
import { useTasks } from "../../state/tasks";
import { notify } from "../../state/notices";
import { runPhashBackfill, toolAction, useToolProgress } from "../../state/tools/library-actions";
import { Button } from "../../ui/controls";
import { ToolCard, ToolPage, ToolError, ToolProgress } from "./shared";
export default function LibraryMaintenanceTool() {
  const snapshot = useLibrary(state => state.snapshot), busy = useTasks(state => state.busy), phash = useToolProgress(state => state.phash);
  return <ToolPage><ToolCard className="rt-row"><RefreshCw size={22} /><div><h3>刷新感知哈希</h3><p>为缺少或过期的图片重新计算感知哈希。以图搜图依赖这项数据。</p>{phash && <ToolProgress value={phash.processed} total={phash.total} />}</div><Button variant="primary" disabled={busy} onClick={() => void runPhashBackfill()}>{phash ? "正在计算…" : "开始刷新"}</Button></ToolCard>
    <ToolCard className="rt-row"><FolderOpen size={22} /><div><h3>失败图片目录</h3><p>查看导入时因元数据异常而被移出的图片，便于手动检查和整理。</p><code>{snapshot?.rejectedImagesDirectory}</code></div><Button disabled={busy} onClick={() => void toolAction("打开目录", async () => { await openRejectedImagesDirectory(); notify("已在文件管理器中打开失败图片目录。"); })}>打开目录</Button></ToolCard><ToolError error={snapshot?.startupError} /></ToolPage>;
}
