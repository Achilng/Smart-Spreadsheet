import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ExternalLink } from "lucide-react";
import { notify } from "../../state/notices";
import { errorText } from "../../../lib/utils/format";

export function StyleWebTools({ compact = false }: { compact?: boolean }) {
  const [opening, setOpening] = useState<string | null>(null);
  const defaults = { extractor: import.meta.env.VITE_STYLE_EXTRACTOR_URL || "", review: import.meta.env.VITE_STYLE_REVIEW_URL || "" };
  const [addresses, setAddresses] = useState(() => ({ extractor: localStorage.getItem("style-cloud-extractor") || defaults.extractor, review: localStorage.getItem("style-cloud-review") || defaults.review }));
  const [configuring, setConfiguring] = useState(!addresses.extractor || !addresses.review);
  async function launch(tool: "extractor" | "review") {
    if (opening) return;
    setOpening(tool);
    try {
      const url = localStorage.getItem("style-cloud-"+tool) || addresses[tool];
      if (!url) { setConfiguring(true); return; }
      await invoke("open_style_web_tool", { tool, url });
    }
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
    onClick={() => void launch(entry.id)} title="在默认浏览器打开服务器工作台"
  ><span className="rt-nav-icon"><ExternalLink size={15} /></span><span><strong>{opening === entry.id ? "正在打开…" : entry.label}</strong>{compact && <small>{entry.description}</small>}</span></button>)}
  <button type="button" className="btn btn-ghost" onClick={()=>setConfiguring(!configuring)}>服务器地址</button>
  {configuring && <div style={{width:"100%",padding:8}}>{entries.map(entry=><label key={entry.id} style={{display:"block",fontSize:12}}>{entry.label}<input className="r-input" type="url" placeholder="https://…" value={addresses[entry.id]} onChange={e=>{const value=e.target.value;setAddresses(current=>({...current,[entry.id]:value}));localStorage.setItem("style-cloud-"+entry.id,value)}} /></label>)}</div>}
  </div>;
}
