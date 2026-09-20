import { useLibrary } from "../../state/library";
import { useTasks } from "../../state/tasks";
import { chooseMigration, resetDataWithConfirmation, useToolProgress } from "../../state/tools/library-actions";
import { Button } from "../../ui/controls";
import { ToolCard, ToolPage, ToolProgress } from "./shared";
export default function DataManagementTool() {
  const snapshot = useLibrary(state => state.snapshot), busy = useTasks(state => state.busy), migration = useToolProgress(state => state.migration);
  return <ToolPage><ToolCard className="rt-row"><div><small>数据目录</small><h3>迁移资料库</h3><p>将数据库、受管图片和缓存复制并校验到一个新的空文件夹，成功后自动切换。</p><code>{snapshot?.dataDirectory}</code></div><Button disabled={busy} onClick={() => void chooseMigration()}>选择迁移目标…</Button></ToolCard>
    {migration && <ToolProgress value={migration.completed} total={migration.total} label={`正在迁移 · ${migration.stage}`} />}
    <ToolCard className="rt-row rt-danger"><div><small>危险操作</small><h3>重置表格</h3><p>清空数据库、受管图片和缩略图，回到初始导入页面。外部原始图片不会被删除。</p></div><Button variant="danger" disabled={busy} onClick={() => void resetDataWithConfirmation()}>重置表格…</Button></ToolCard></ToolPage>;
}
