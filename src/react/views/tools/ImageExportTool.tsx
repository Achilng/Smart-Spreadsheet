import { useEffect, useRef, useState } from "react";
import { emitTo, listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { Folder, FolderInput, Trash2, X } from "lucide-react";
import { collectExportImages, exportSelectedImages, getImageExportSettings, setImageExportSettings, type ExportProgress, type ImageExportSettings, type ImageFilesExportResult } from "../../../lib/api";
import { focusMainWindow, type ToolboxSelectionSnapshot } from "../../../lib/windows/toolbox";
import { errorText, formatCount } from "../../../lib/utils/format";
import { runTask, useTasks } from "../../state/tasks";
import { notify } from "../../state/notices";
import { useNativeDrop } from "../../state/tools/use-native-drop";
import { Button, Checkbox, Input } from "../../ui/controls";
import { ToolCard, ToolPage, ToolError, ToolProgress, fileName } from "./shared";

const defaults: ImageExportSettings = { destination: null, renameEnabled: false, renameMode: "random", customName: "", stripMetadata: false };
export default function ImageExportTool({ active }: { active: boolean }) {
  const [selection, setSelection] = useState<ToolboxSelectionSnapshot | null>(null), [settings, setSettings] = useState(defaults), [ready, setReady] = useState(false), [listenerReady, setListenerReady] = useState(false), [paths, setPaths] = useState<string[]>([]), [scanning, setScanning] = useState(false), [exporting, setExporting] = useState(false), [progress, setProgress] = useState<ExportProgress | null>(null), [result, setResult] = useState<ImageFilesExportResult | null>(null), [error, setError] = useState<string | null>(null);
  const busy = useTasks(state => state.busy), zone = useRef<HTMLButtonElement>(null), pending = useRef(false), saveChain = useRef(Promise.resolve());
  const count = (selection?.count ?? 0) + paths.length, renameMode = settings.renameEnabled ? settings.renameMode : "original", validName = renameMode !== "custom" || !!settings.customName.trim(), canExport = !busy && !scanning && !exporting && count > 0 && !!settings.destination && validName;
  async function requestSelection() { try { await emitTo("main", "toolbox://request-selection"); } catch { setError("无法读取主窗口选区，请确认主窗口仍在运行。"); } }
  useEffect(() => {
    let disposed = false; let cleanup: (() => void) | undefined;
    void getImageExportSettings().then(value => { if (!disposed) { setSettings(value); setReady(true); } }).catch(cause => { if (!disposed) setError(`无法读取已保存的导出设置：${errorText(cause)}`); });
    void listen<ToolboxSelectionSnapshot>("main://selection-changed", event => { if (!disposed) setSelection(event.payload); }).then(fn => { if (disposed) fn(); else { cleanup = fn; setListenerReady(true); } }).catch(cause => { if (!disposed) setError(`无法监听主窗口选区：${errorText(cause)}`); });
    return () => { disposed = true; cleanup?.(); };
  }, []);
  useEffect(() => { if (active && listenerReady) void requestSelection(); }, [active, listenerReady]);
  useEffect(() => {
    if (!ready) return;
    const timer = setTimeout(() => { saveChain.current = saveChain.current.catch(() => {}).then(() => setImageExportSettings(settings)).catch(cause => setError(`无法保存导出设置：${errorText(cause)}`)); }, 250);
    return () => clearTimeout(timer);
  }, [settings, ready]);
  async function add(pathsToAdd: string[]) {
    if (!pathsToAdd.length || pending.current || busy) return; pending.current = true; setScanning(true); setError(null); setResult(null);
    try { await runTask("扫描导出图片", async () => setPaths(await collectExportImages([...paths, ...pathsToAdd]))); } catch (cause) { setError(errorText(cause)); } finally { pending.current = false; setScanning(false); }
  }
  const dragging = useNativeDrop(active, busy || scanning || exporting, zone, add, setError);
  async function choose(destination: boolean) { try { const value = await open({ directory: true, multiple: false, title: destination ? "选择图片导出文件夹" : "选择需要导出的图片文件夹" }); if (typeof value !== "string") return; if (destination) { setSettings(valueBefore => ({ ...valueBefore, destination: value })); setResult(null); setError(null); } else await add([value]); } catch (cause) { setError(errorText(cause)); } }
  async function runExport() {
    if (!canExport || !settings.destination || pending.current) return; pending.current = true; setExporting(true); setProgress(null); setResult(null); setError(null);
    const destination = settings.destination;
    try { await runTask("导出图片", async () => { let cleanup: (() => void) | undefined; try {
      cleanup = await listen<ExportProgress>("export://progress", event => setProgress(event.payload));
      const output = await exportSelectedImages(selection?.selection ?? { kind: "explicit", rowIds: [] }, paths, destination, renameMode, renameMode === "custom" ? settings.customName.trim() : null, settings.stripMetadata);
      setResult(output); notify(`已导出 ${formatCount(output.exported)} 张图片${output.missing ? `，${formatCount(output.missing)} 张源文件不可用` : ""}。`); await requestSelection();
    } finally { cleanup?.(); setProgress(null); } }); } catch (cause) { setError(errorText(cause)); } finally { pending.current = false; setExporting(false); }
  }
  const update = (patch: Partial<ImageExportSettings>) => { setSettings(value => ({ ...value, ...patch })); setResult(null); };
  return <ToolPage><ToolCard><div className="rt-row"><span className="rt-step">1</span><div><h3>选择需要导出的图片</h3><p>{count ? `共选择 ${formatCount(count)} 张图片（主窗口 ${formatCount(selection?.count ?? 0)} 张，另行添加 ${formatCount(paths.length)} 张）` : "可沿用主窗口选区，也可直接选择文件夹或拖入图片。"}</p></div></div><div className="rt-actions"><Button onClick={() => void focusMainWindow().catch(cause => setError(`无法切换到主窗口：${errorText(cause)}`))}>返回主窗口选择</Button><button ref={zone} className={`rt-source-drop${dragging ? " is-dragging" : ""}`} disabled={busy || scanning || exporting} onClick={() => void choose(false)}><FolderInput size={20} /><span><strong>{scanning ? "正在扫描图片…" : "点击选择文件夹"}</strong><small>或拖入图片 / 文件夹</small></span></button></div>{paths.length > 0 && <><div className="rt-row"><small>已追加并去重 {formatCount(paths.length)} 张，文件夹已包含全部子文件夹</small><Button variant="ghost" disabled={busy} onClick={() => { setPaths([]); setResult(null); }}><Trash2 size={14} />清空</Button></div><div className="rt-chips">{paths.slice(0, 4).map(path => <span key={path} title={path}><code>{fileName(path)}</code><Button variant="ghost" size="icon" disabled={busy} aria-label={`移除 ${fileName(path)}`} onClick={() => { setPaths(values => values.filter(value => value !== path)); setResult(null); }}><X size={13} /></Button></span>)}{paths.length > 4 && <small>还有 {formatCount(paths.length - 4)} 张</small>}</div></>}</ToolCard>
    <ToolCard><div className="rt-row"><span className="rt-step">2</span><div><h3>选择导出位置 <small>必选</small></h3><p>图片会直接写入所选文件夹；遇到同名文件时自动追加序号，不会覆盖已有文件。</p></div></div><Button className="rt-folder" disabled={busy} onClick={() => void choose(true)}><Folder size={22} /><span><strong>{settings.destination ? fileName(settings.destination) : "选择导出文件夹…"}</strong><small>{settings.destination ?? "尚未选择"}</small></span><span>{settings.destination ? "更换" : "选择"}</span></Button></ToolCard>
    <ToolCard><div className="rt-row"><span className="rt-step">3</span><div><h3>文件名</h3><p>默认保留原文件名；同名文件会自动追加序号。</p></div><label><Checkbox checked={settings.renameEnabled} disabled={busy} onCheckedChange={value => update({ renameEnabled: value === true })} />重命名</label></div>{settings.renameEnabled && <><div className="rt-fields"><label><input type="radio" name="export-rename" checked={settings.renameMode === "random"} disabled={busy} onChange={() => update({ renameMode: "random" })} />随机乱码，例如 a4f083bd7c19e260.png</label><label><input type="radio" name="export-rename" checked={settings.renameMode === "custom"} disabled={busy} onChange={() => update({ renameMode: "custom" })} />自定义命名，按“名称_1、名称_2…”顺序生成</label></div>{settings.renameMode === "custom" && <label className="rt-field">文件名前缀<Input value={settings.customName} disabled={busy} maxLength={120} placeholder="例如：胡桃精选" onChange={event => update({ customName: event.target.value })} /><small>{settings.customName.trim() || "自定义名称"}_1.png</small></label>}</>}</ToolCard>
    <ToolCard><div className="rt-row"><span className="rt-step">4</span><div><h3>图片元数据</h3><p>重新编码导出副本，移除 PNG 附加块及 NovelAI Alpha 通道隐写元数据。</p></div><label><Checkbox disabled={busy} checked={settings.stripMetadata} onCheckedChange={value => update({ stripMetadata: value === true })} />抹除元数据</label></div>{settings.stripMetadata && <p className="rt-warning">只处理新导出的图片副本；会微调透明度最低位（肉眼不可见），原图和资料库不会被修改。</p>}</ToolCard><ToolError error={error} />{exporting && progress && <ToolProgress value={progress.processed} total={progress.total} label="正在导出图片…" />}{result && <ToolCard><h3>导出完成</h3><p>已导出 {formatCount(result.exported)} 张{result.missing ? `，${formatCount(result.missing)} 张源文件不可用` : ""}</p><code>{result.directory}</code></ToolCard>}
    <footer className="rt-row rt-card"><div><p>{!count ? "请选择文件夹、拖入图片，或在主窗口选择图片" : !settings.destination ? "请选择导出文件夹" : !validName ? "请输入自定义文件名前缀" : `准备导出 ${formatCount(count)} 张图片`}</p>{ready && <small>导出位置和选项会自动记住</small>}</div><Button variant="primary" disabled={!canExport} onClick={() => void runExport()}>{exporting ? "正在导出…" : "开始导出"}</Button></footer></ToolPage>;
}
