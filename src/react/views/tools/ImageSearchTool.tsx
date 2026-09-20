import { useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ImageUp } from "lucide-react";
import { getRowsByIds, searchSimilarImages, type RowRecord, type SimilarImageMatch } from "../../../lib/api";
import { errorText, formatCount } from "../../../lib/utils/format";
import { openRowInMainWindow } from "../../../lib/windows/toolbox";
import { useNativeDrop } from "../../state/tools/use-native-drop";
import { notify } from "../../state/notices";
import { runTask, useTasks } from "../../state/tasks";
import { Thumbnail } from "../../ui/Thumbnail";
import { Button } from "../../ui/controls";
import { ToolCard, ToolError, ToolPage, fileName } from "./shared";
const extensions = ["png", "jpg", "jpeg", "bmp", "gif", "webp", "tif", "tiff"];
export default function ImageSearchTool({ active }: { active: boolean }) {
  const [path, setPath] = useState<string | null>(null), [matches, setMatches] = useState<SimilarImageMatch[]>([]), [rows, setRows] = useState<Map<number, RowRecord>>(new Map()), [searching, setSearching] = useState(false), [searched, setSearched] = useState(false), [error, setError] = useState<string | null>(null), [opening, setOpening] = useState<number | null>(null);
  const zone = useRef<HTMLDivElement>(null), pending = useRef(false), busy = useTasks(state => state.busy);
  async function search(value: string) {
    if (pending.current || busy) return; pending.current = true; setPath(value); setMatches([]); setRows(new Map()); setSearched(false); setSearching(true); setError(null);
    try { await runTask("搜索相似图片", async () => { const result = await searchSimilarImages(value, 10); const records = result.length ? await getRowsByIds(result.map(item => item.rowId)) : []; setMatches(result); setRows(new Map(records.map(row => [row.id, row]))); setSearched(true); }); }
    catch (cause) { setError(errorText(cause)); } finally { pending.current = false; setSearching(false); }
  }
  const dragging = useNativeDrop(active, busy || searching, zone, async paths => { if (paths.length !== 1 || !extensions.includes(paths[0].split(".").pop()?.toLowerCase() ?? "")) { setError(paths.length > 1 ? "请一次只拖入一张参考图片。" : "请拖入支持的图片文件（PNG、JPG、BMP、GIF、WebP 或 TIFF）。"); return; } await search(paths[0]); }, setError);
  async function choose() { try { const value = await open({ multiple: false, directory: false, title: "选择用于搜索的图片", filters: [{ name: "图片", extensions }] }); if (typeof value === "string") await search(value); } catch (cause) { setError(errorText(cause)); } }
  return <ToolPage><div ref={zone} className={dragging ? "rt-drop is-dragging" : "rt-drop"}><ToolCard className="rt-row"><ImageUp size={26} /><div><h3>{dragging ? "松开即可开始搜索" : "选择或拖入一张参考图片"}</h3><p>工具会计算参考图的感知哈希，并返回距离不超过 10 的库内图片。</p><small>支持 PNG、JPG、BMP、GIF、WebP 和 TIFF</small></div><Button variant="primary" disabled={busy || searching} onClick={() => void choose()}>{searching ? "搜索中…" : path ? "换一张图片" : "选择图片…"}</Button></ToolCard></div>{path && <p title={path}>参考图片　{fileName(path)}</p>}<ToolError error={error} />{searching ? <div className="rt-empty" role="status">正在搜索相似图片…</div> : searched && !matches.length ? <div className="rt-empty"><strong>没有找到相似图片</strong><p>可以先到“资料库维护”刷新感知哈希后再试。</p></div> : matches.length > 0 && <><h3>搜索结果 · {formatCount(matches.length)} 张</h3><div className="rt-image-grid">{matches.map(match => { const row = rows.get(match.rowId); const name = fileName(row?.imagePath ?? row?.storedImagePath ?? `图片 #${match.rowId}`); return <button className="rt-image-result" title="在主窗口画廊中定位" key={match.rowId} disabled={opening !== null} onClick={() => { setOpening(match.rowId); void openRowInMainWindow(match.rowId).catch(cause => notify(`无法在主窗口打开图片：${errorText(cause)}`, "error")).finally(() => setOpening(null)); }}><Thumbnail rowId={match.rowId} alt={name} /><strong title={name}>{name}</strong><small>{opening === match.rowId ? "正在主窗口中打开…" : `${match.distance === 0 ? "完全匹配" : match.distance <= 3 ? "极高相似" : match.distance <= 6 ? "高度相似" : "可能相似"} · 距离 ${match.distance}`}</small></button>; })}</div></>}</ToolPage>;
}
