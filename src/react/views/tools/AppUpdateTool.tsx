import { useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { confirm } from "@tauri-apps/plugin-dialog";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { Download, RefreshCw } from "lucide-react";
import { errorText } from "../../../lib/utils/format";
import { useTasks, runTask } from "../../state/tasks";
import { useHistory } from "../../state/history";
import { notify } from "../../state/notices";
import { Button } from "../../ui/controls";
import { ToolCard, ToolError, ToolPage, ToolProgress } from "./shared";

type Status = "idle" | "checking" | "latest" | "available" | "downloading" | "installing" | "error";
export default function AppUpdateTool() {
  const [version, setVersion] = useState("读取中…"), [status, setStatus] = useState<Status>("idle"), [update, setUpdate] = useState<Update | null>(null);
  const [checked, setChecked] = useState<Date | null>(null), [error, setError] = useState(""), [downloaded, setDownloaded] = useState(0), [total, setTotal] = useState<number | null>(null);
  const resource = useRef<Update | null>(null), mounted = useRef(false), operation = useRef(false);
  const tasks = useTasks(), history = useHistory(), working = ["checking", "downloading", "installing"].includes(status);
  const blocked = tasks.busy || history.busy ? "还有任务正在进行，请等待任务结束后再更新。" : history.undoCount + history.redoCount > 0 ? "工具箱仍有可撤回或重做的修改。请关闭并重新打开工具箱，确认放弃这些记录后再更新。" : null;
  useEffect(() => {
    mounted.current = true; let disposed = false;
    void getVersion().then(value => { if (!disposed) setVersion(value); }).catch(error => { if (!disposed) { setVersion("未知"); notify(`无法读取当前版本：${errorText(error)}`, "error"); } });
    return () => { disposed = true; mounted.current = false; const current = resource.current; resource.current = null; void current?.close().catch(() => {}); };
  }, []);
  async function checkUpdate() {
    if (operation.current) return; operation.current = true; setStatus("checking"); setError(""); setDownloaded(0); setTotal(null);
    try {
      const previous = resource.current; resource.current = null; setUpdate(null); await previous?.close();
      const next = await check({ timeout: 15_000 });
      if (!mounted.current) { await next?.close(); return; }
      resource.current = next; setUpdate(next); setChecked(new Date()); setStatus(next ? "available" : "latest");
    } catch (cause) { setError(errorText(cause)); setStatus("error"); }
    finally { operation.current = false; }
  }
  async function install() {
    if (!update || operation.current || blocked) return; operation.current = true;
    try {
      if (!await confirm(`将下载并安装智能表格 ${update.version}。安装时主窗口和工具箱会关闭，完成后自动重新打开。是否继续？`, { title: "安装应用更新", kind: "warning", okLabel: "下载并安装", cancelLabel: "取消" })) return;
      if (useHistory.getState().undoCount + useHistory.getState().redoCount > 0) throw new Error("仍有可撤回或重做的修改，请先处理这些记录。");
      await runTask("安装应用更新", async () => {
        setStatus("downloading"); setDownloaded(0); setTotal(null);
        await update.download(event => { if (event.event === "Started") setTotal(event.data.contentLength ?? null); else if (event.event === "Progress") setDownloaded(value => value + event.data.chunkLength); }, { timeout: 120_000 });
        setStatus("installing"); await update.install(); await relaunch();
      });
    } catch (cause) { setError(errorText(cause)); setStatus("error"); notify(`更新安装失败：${errorText(cause)}`, "error"); }
    finally { operation.current = false; }
  }
  return <ToolPage><ToolCard className="rt-row"><div><small>当前版本</small><h3>智能表格 {version}</h3><p>仅在你点击按钮后连接 GitHub Release，不会在启动时自动联网。</p>{checked && <small>最近检查：{checked.toLocaleTimeString("zh-CN")}</small>}</div><Button disabled={working} onClick={() => void checkUpdate()}><RefreshCw size={15} />{status === "checking" ? "正在检查…" : status === "idle" ? "检查更新" : "重新检查"}</Button></ToolCard>
    {status === "latest" && <ToolCard><h3>已经是最新版本</h3><p>GitHub Release 暂时没有比 {version} 更新的版本。</p></ToolCard>}
    {status === "available" && update && <ToolCard><div className="rt-row"><div><small>发现新版本</small><h3>{update.version}</h3>{update.date && <small>发布于 {new Date(update.date).toLocaleString("zh-CN")}</small>}</div><Button variant="primary" disabled={!!blocked} title={blocked ?? undefined} onClick={() => void install()}><Download size={15} />下载并安装</Button></div>{blocked && <p role="status" className="rt-warning">{blocked}</p>}<h4>更新说明</h4><pre className="rt-notes">{update.body?.trim().replace(/^#{1,6}\s+/gm, "").replace(/^\s*[-*+]\s+/gm, "• ").replace(/\*\*([^*]+)\*\*/g, "$1").replace(/`([^`]+)`/g, "$1") || "这个版本没有附带更新说明。"}</pre></ToolCard>}
    {(status === "downloading" || status === "installing") && <ToolCard><h3>{status === "downloading" ? "正在下载" : "正在安装"} {update?.version}</h3>{total ? <ToolProgress value={downloaded} total={total} label="字节" /> : <p role="status">{status === "downloading" ? `已下载 ${(downloaded / 1048576).toFixed(1)} MB` : "即将重新打开应用…"}</p>}<p>请不要关闭应用。Windows 安装程序接手后，当前窗口会自动退出。</p></ToolCard>}
    {status === "error" && <ToolError error={error} />}<small>更新源：github.com/Achilng/Smart-Spreadsheet · 安装包会先验证更新签名，验证失败不会安装。</small></ToolPage>;
}
