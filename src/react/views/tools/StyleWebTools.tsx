import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ExternalLink } from "lucide-react";
import { notify } from "../../state/notices";
import { errorText } from "../../../lib/utils/format";

export function StyleWebTools({ compact = false }: { compact?: boolean }) {
  const [opening, setOpening] = useState<string | null>(null);
  async function launch(tool: "extractor" | "review") {
    if (opening) return;
    setOpening(tool);
    try { await invoke("open_style_web_tool", { tool }); }
    catch (error) { notify(errorText(error), "error"); }
    finally { setOpening(null); }
  }
  const entries = [
    { id: "extractor" as const, label: "画风提取工作台", description: "批量调用模型，实时查看提取进度" },
    { id: "review" as const, label: "画风批改台", description: "人工核对结果、填写正确答案和备注" },
  ];
  return <div className={compact ? undefined : "rt-actions"}>{entries.map(entry => <button
    key={entry.id} type="button" disabled={opening !== null}
    className={compact ? undefined : "btn"}
    onClick={() => void launch(entry.id)} title="启动本地服务并在默认浏览器打开"
  ><span className="rt-nav-icon"><ExternalLink size={15} /></span><span><strong>{opening === entry.id ? "正在打开…" : entry.label}</strong>{compact && <small>{entry.description}</small>}</span></button>)}</div>;
}
