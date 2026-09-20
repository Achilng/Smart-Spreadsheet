import { useEffect, useMemo, useRef, useState } from "react";
import { Dices } from "lucide-react";
import { getCustomArtists, listDistinctArtists, setCustomArtists } from "../../../lib/api";
import { registerCloseGuard } from "../../../lib/stores/close-guard";
import { errorText, formatCount } from "../../../lib/utils/format";
import { notify } from "../../state/notices";
import { Button, Checkbox, Input, Textarea } from "../../ui/controls";
import { ToolCard, ToolPage, ToolError } from "./shared";

export default function ArtistGeneratorView() {
  const [artists, setArtists] = useState<string[]>([]), [custom, setCustom] = useState(""), [library, setLibrary] = useState(true), [useCustom, setUseCustom] = useState(true), [clean, setClean] = useState(true), [count, setCount] = useState(3), [result, setResult] = useState(""), [draw, setDraw] = useState(0), [loading, setLoading] = useState(true), [error, setError] = useState<string | null>(null);
  const latest = useRef(""), saved = useRef(""), pending = useRef<Promise<void> | null>(null), timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined), resultRef = useRef<HTMLTextAreaElement>(null), diceRef = useRef<HTMLSpanElement>(null);
  const pool = useMemo(() => [...new Set([...(library ? artists : []), ...(useCustom ? custom.split(/\r?\n/).map(s => s.trim()).filter(Boolean) : [])])].filter(s => !clean || !s.includes("::")), [library, artists, useCustom, custom, clean]);
  async function flush() {
    if (pending.current) { await pending.current; if (latest.current !== saved.current) await flush(); return; }
    if (latest.current === saved.current) return;
    const value = latest.current;
    pending.current = setCustomArtists(value).then(() => { saved.current = value; }).catch(cause => { notify(`保存自定义名单失败：${errorText(cause)}`, "error"); }).finally(() => { pending.current = null; });
    await pending.current;
  }
  useEffect(() => {
    let disposed = false;
    void Promise.all([listDistinctArtists(), getCustomArtists()]).then(([list, text]) => { if (!disposed) { setArtists(list); setCustom(text); latest.current = saved.current = text; setLoading(false); } }).catch(cause => { if (!disposed) { setError(errorText(cause)); setLoading(false); } });
    const guard = registerCloseGuard(async () => { clearTimeout(timer.current); await flush(); return latest.current !== saved.current ? "自定义画师名单尚未保存" : null; });
    return () => { disposed = true; clearTimeout(timer.current); void flush(); guard(); };
  }, []);
  useEffect(() => {
    if (!draw || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const animations = [resultRef.current?.animate([{ opacity: .55 }, { opacity: 1 }], { duration: 280 }), diceRef.current?.animate([{ transform: "rotate(0deg)" }, { transform: "rotate(360deg)" }], { duration: 420, easing: "ease-out" })];
    return () => animations.forEach(animation => animation?.cancel());
  }, [draw]);
  function generate() { const values = [...pool]; for (let i = values.length - 1; i > 0; i--) { const j = Math.floor(Math.random() * (i + 1)); [values[i], values[j]] = [values[j], values[i]]; } setResult(values.slice(0, Math.min(Math.max(1, Math.floor(count) || 1), values.length)).join(", ")); setDraw(n => n + 1); }
  return <ToolPage narrow><ToolCard>{loading ? <p role="status">正在加载画师池…</p> : error ? <ToolError error={error} /> : <><p>从启用的来源随机抽取画师拼成提示词串，复制后可直接喂给 NovelAI。</p><div className="rt-fields"><label><Checkbox checked={library} onCheckedChange={value => setLibrary(value === true)} />库内画师（{formatCount(artists.length)} 个）</label><label><Checkbox checked={useCustom} onCheckedChange={value => setUseCustom(value === true)} />自定义名单</label><label><Checkbox checked={clean} onCheckedChange={value => setClean(value === true)} />只用干净 artist: 片段（去掉带 :: 权重的）</label></div>{useCustom && <label className="rt-field">自定义名单（一行一个，自动保存）<Textarea rows={4} value={custom} placeholder={"artist:wlop\nartist:ask"} onChange={event => { const value = event.target.value; setCustom(value); latest.current = value; clearTimeout(timer.current); timer.current = setTimeout(() => void flush(), 600); }} /></label>}<div className="rt-row"><label>数量 <Input type="number" min={1} value={count} onChange={event => setCount(event.target.valueAsNumber || 1)} className="rt-number" /></label><small>当前池 {formatCount(pool.length)} 个画师</small><Button variant="primary" disabled={!pool.length} onClick={generate}><span ref={diceRef}><Dices size={16} /></span>{result ? "再来一个" : "生成"}</Button></div><textarea className="r-input r-textarea" ref={resultRef} aria-label="随机画师串结果" rows={3} value={result} readOnly placeholder="点击生成，随机画师串会显示在这里" /><div className="rt-actions"><Button disabled={!result} onClick={() => void navigator.clipboard.writeText(result).then(() => notify("已复制画师串到剪贴板。")).catch(() => notify("复制失败，请检查剪贴板权限。", "error"))}>复制</Button></div></>}</ToolCard></ToolPage>;
}
