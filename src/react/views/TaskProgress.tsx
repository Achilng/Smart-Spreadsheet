import { useTasks } from "../state/tasks";
import { useMaintenance } from "../state/library-session";

const stages: Record<string, string> = { extracting: "解压图片", scanning: "扫描文件", hashing: "检查重复", processing: "读取元数据", perceptualHashing: "计算图片指纹", copying: "保存图片" };
export function TaskProgress() {
  const task = useTasks(), maintenance = useMaintenance();
  const { label, progress } = task.busy ? task : maintenance;
  const busy = task.busy || Boolean(maintenance.label);
  if (!busy) return null;
  const percent = progress && progress.total > 0 ? Math.min(100, Math.round(progress.processed / progress.total * 100)) : null;
  return <div className="r-task-progress" role="status" aria-live="polite"><span>{progress?.stage ? stages[progress.stage] ?? label : label}</span>
    <progress max={100} value={percent ?? undefined} aria-label={label} />
    <small>{progress && progress.total > 0 ? `${progress.processed} / ${progress.total}` : "处理中…"}</small></div>;
}
