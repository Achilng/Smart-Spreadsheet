import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import { notify } from "../state/notices";
import { errorText } from "../../lib/utils/format";

export function WindowControls() {
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    const appWindow = getCurrentWindow();
    const sync = () => { void appWindow.isMaximized().then(value => { if (!disposed) setMaximized(value); }).catch(() => {}); };
    sync();
    void appWindow.onResized(sync).then(unlisten => { if (disposed) unlisten(); else stop = unlisten; }).catch(() => {});
    return () => { disposed = true; stop?.(); };
  }, []);
  const run = (action: "minimize" | "toggleMaximize" | "close") => {
    void getCurrentWindow()[action]().catch(error => notify(errorText(error), "error"));
  };
  return <div className="r-window-controls">
    <button type="button" aria-label="最小化" title="最小化" onClick={() => run("minimize")}><Minus size={13} strokeWidth={1.5} /></button>
    <button type="button" aria-label={maximized ? "还原" : "最大化"} title={maximized ? "还原" : "最大化"} onClick={() => run("toggleMaximize")}>{maximized ? <Copy size={13} strokeWidth={1.5} /> : <Square size={13} strokeWidth={1.5} />}</button>
    <button type="button" className="close" aria-label="关闭" title="关闭" onClick={() => run("close")}><X size={13} strokeWidth={1.5} /></button>
  </div>;
}
