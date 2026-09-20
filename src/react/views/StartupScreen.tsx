import { initializeLibrary, useLibrary } from "../state/library";
import { chooseDirectory, resetAndReconfigure, runStartupMaintenance } from "../state/library-session";
import { useTasks } from "../state/tasks";
import { Button } from "../ui/controls";
import { WindowControls } from "../ui/WindowControls";

export function StartupScreen() {
  const { loaded, snapshot, error } = useLibrary(), busy = useTasks(state => state.busy);
  const failure = error || snapshot?.startupError;
  return <div className="r-flow"><header className="r-flow-header" data-tauri-drag-region><strong data-tauri-drag-region>智能表格</strong><WindowControls /></header>
    <div className="r-state-message">{!loaded ? <p role="status">正在读取应用状态…</p> : <div className="r-startup-card">
      <h1>{failure ? "无法打开数据目录" : "开始整理图片"}</h1>
      <p>{failure || "选择数据目录来保存图片资料、缩略图和整理结果，也可以打开已有的智能表格资料库。"}</p>
      <div>{failure ? <><Button disabled={busy} onClick={() => void initializeLibrary().then(runStartupMaintenance)}>重试</Button><Button disabled={busy} variant="primary" onClick={() => void resetAndReconfigure()}>重新配置</Button></> : <><Button disabled={busy} variant="primary" onClick={() => void chooseDirectory("initialize")}>新建数据目录</Button><Button disabled={busy} onClick={() => void chooseDirectory("open")}>打开已有数据目录</Button></>}</div>
      {failure && <small>重新配置会清除当前目录定位信息，保留原始资料文件。</small>}
    </div>}</div>
  </div>;
}
