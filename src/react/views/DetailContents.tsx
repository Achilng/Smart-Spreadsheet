import { useEffect, useRef, useState } from "react";
import { LlmBadge } from "../ui/LlmBadge";
import { Check, Copy, Filter, FolderOpen, ImageOff, Maximize2, X } from "lucide-react";
import { createTag, mutableRowState, setTagsForRow, type RowRecord } from "../../lib/api";
import { rowFileName, rowResolution } from "../../lib/utils/row-display";
import { modelVersionBadge } from "../../lib/utils/model-version";
import { errorText } from "../../lib/utils/format";
import { vibeStatuses } from "../../lib/images/vibe-statuses";
import { notifyToolboxLibraryChanged } from "../../lib/windows/library-events";
import { patchRowFields, refreshTags, useLibrary } from "../state/library";
import { useImageCache } from "../state/image-cache";
import { notify } from "../state/notices";
import { runTask, useTasks } from "../state/tasks";
import { recordRowStateChange } from "../state/history";
import { assignToGroup, invalidateGroups } from "../state/groups";
import { filterByArtists, showRowInExplorer } from "../state/row-actions";
import { beginFileDrag } from "../state/file-drag";
import { Button, Hint, Input } from "../ui/controls";
import { FieldEditor } from "../ui/FieldEditor";
import { useProgressiveImage } from "../ui/use-image";
import { DetailLightbox } from "../ui/DetailLightbox";
import "../ui/detail-images.css";

export function DetailContents({ row }: { row: RowRecord }) {
  const [lightbox, setLightbox] = useState(false);
  const [vibeCount, setVibeCount] = useState(row.vibeReferenceCount);
  const [decoded, setDecoded] = useState(true);
  const [copied, setCopied] = useState<string | null>(null);
  const copyTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const drag = useRef(false);
  const busy = useTasks(state => state.busy);
  const version = useImageCache(state => state.version);
  const hasImage = Boolean(row.imagePath?.trim() || row.storedImagePath?.trim());
  const image = useProgressiveImage(hasImage ? row.id : undefined);
  const badge = modelVersionBadge(row.generationModel);
  useEffect(() => {
    let disposed = false;
    if (hasImage) void vibeStatuses.load(row.id).then(count => { if (!disposed) setVibeCount(count); }, () => {});
    return () => { disposed = true; };
  }, [row.id, hasImage, version]);
  useEffect(() => { setDecoded(true); }, [image.url]);
  useEffect(() => () => clearTimeout(copyTimer.current), []);
  const copy = async (label: string, text: string) => {
    try { await navigator.clipboard.writeText(text); setCopied(label); clearTimeout(copyTimer.current); copyTimer.current = setTimeout(() => setCopied(null), 1200); }
    catch (error) { notify(`复制失败：${errorText(error)}`, "error"); }
  };
  const ungroup = async () => {
    try { await runTask("取消分组", async () => { await assignToGroup({ kind: "explicit", rowIds: [row.id] }, null); patchRowFields(row.id, { groupId: null, groupName: null }); notifyToolboxLibraryChanged("main"); }); }
    catch (error) { notify(`取消分组失败：${errorText(error)}`, "error"); }
  };
  return <><div className="r-detail-scroll rd-contents">
    <div className="rd-preview"><button className="rd-preview-button" type="button" aria-label={`放大预览：${rowFileName(row) || `第 ${row.sourceOrdinal} 行图片`}`} disabled={!hasImage} onMouseDown={event => { if (hasImage) beginFileDrag(event.nativeEvent, row.id, () => { drag.current = true; }, () => { setTimeout(() => { drag.current = false; }, 0); }); }} onClick={() => { if (!drag.current) setLightbox(true); }}>
      {image.url && decoded ? <img src={image.url} alt={rowFileName(row) || "图片预览"} draggable={false} onError={() => setDecoded(false)} /> : <span className="rd-preview-empty">{!hasImage ? <><ImageOff size={22} />无图片</> : image.error || !decoded ? <><ImageOff size={22} />图片不可用</> : "正在加载图片…"}</span>}
      <span className="rd-preview-badges">{badge && <span className={`version-badge ${badge.className}`} title={`作画模型：${row.generationModel}`}>{badge.label}</span>}{Boolean(vibeCount) && <span className="vibe-badge" title={`原图元数据包含 ${vibeCount} 个 VIBE 引用，拖到 NovelAI 可一并导入`}>VIBE ×{vibeCount}</span>}</span>{hasImage && <span className="rd-preview-open"><Maximize2 size={13} />查看原图</span>}
    </button>{image.error && <div className="rd-preview-retry"><span>{image.url ? "高清预览加载失败" : "预览加载失败"}</span><Button size="sm" variant="ghost" onClick={image.retry}>重新加载</Button></div>}</div>
    <FieldEditor row={row} label="备注" field="note" />
    <TagEditor row={row} />
    <section><h4>分组</h4><div className="rd-group">{row.groupName ? <><span title={row.groupName}>{row.groupName}</span><Button variant="ghost" size="sm" disabled={busy} onClick={() => void ungroup()}>取消分组</Button></> : <p className="r-muted">未分组</p>}</div></section>
    {([["正向提示词", "positivePrompt"], ["角色提示词", "characterPrompt"], ["负向提示词", "negativePrompt"]] as const).map(([label, field]) => <FieldEditor key={field} row={row} label={label} field={field} />)}
    <section><h4>生成信息</h4><dl className="r-parameters">{[
      ["模型", row.generationModel], ["采样器", row.generationSampler], ["步数", row.generationSteps], ["种子", row.generationSeed], ["CFG", row.generationScale], ["CFG Rescale", row.generationCfgRescale], ["噪声调度", row.generationNoiseSchedule], ["尺寸", rowResolution(row)], ["时间", row.time],
    ].filter(([, value]) => value !== null && value !== undefined && value !== "").map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>{row.metadataFailed && <p className="rd-metadata-warning">这张图片的生成信息未能完整解析。</p>}</section>
    {([{ label: "画师串", value: row.artists }, { label: "图片文件夹", value: row.imageFolder }, { label: "图片路径", value: row.imagePath }] as const).map(field => <section key={field.label}><div className="r-section-heading"><h4>{field.label} {field.label === "画师串" && <LlmBadge source={row.artistLlm} />}</h4><div className="r-field-actions">{field.label === "画师串" && field.value && <Hint text="筛选相同画师串"><Button size="icon" variant="ghost" aria-label="筛选相同画师串" onClick={() => filterByArtists(field.value!)}><Filter size={13} /></Button></Hint>}{field.label === "图片路径" && hasImage && <Hint text="在资源管理器中定位"><Button size="icon" variant="ghost" aria-label="在资源管理器中定位" onClick={() => void showRowInExplorer(row)}><FolderOpen size={13} /></Button></Hint>}<Hint text={`复制${field.label}`}><Button size="icon" variant="ghost" disabled={!field.value} aria-label={`复制${field.label}`} onClick={() => void copy(field.label, field.value!)}>{copied === field.label ? <Check size={13} /> : <Copy size={13} />}</Button></Hint></div></div><p className={field.value ? "r-prompt" : "r-muted"}>{field.value || (field.label === "画师串" && row.artistLlm ? "未识别到画风" : "—")}</p></section>)}
  </div>{lightbox && <DetailLightbox row={row} onClose={() => setLightbox(false)} />}</>;
}

function TagEditor({ row }: { row: RowRecord }) {
  const tags = useLibrary(state => state.tags);
  const busy = useTasks(state => state.busy);
  const [query, setQuery] = useState("");
  const [focused, setFocused] = useState(false);
  const [choice, setChoice] = useState(-1);
  const [error, setError] = useState<string | null>(null);
  const suggestions = tags.filter(tag => !row.tags.includes(tag.name) && tag.name.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));
  const save = async (next: string[], create?: string) => {
    if (busy) return;
    setError(null);
    try {
      await runTask("保存 Tag", async () => {
        const before = mutableRowState(row);
        if (create && !tags.some(tag => tag.name === create)) await createTag(create);
        const result = await setTagsForRow(row.id, next);
        patchRowFields(row.id, { tags: result.normalizedTags });
        setQuery(""); setChoice(-1);
        notifyToolboxLibraryChanged("main");
        await refreshTags(); invalidateGroups();
        try { await recordRowStateChange(create ? "添加 Tag" : "编辑 Tag", [before]); }
        catch (historyError) { notify(`操作已完成，但未能记录撤销历史：${errorText(historyError)}`, "error"); }
      });
    } catch (failure) { setError(errorText(failure)); }
  };
  const add = (name: string) => { const normalized = name.trim(); if (normalized && !row.tags.includes(normalized)) void save([...row.tags, normalized], normalized); };
  return <section className="rd-tags"><h4>Tags</h4><div className="rd-tag-chips">{row.tags.length ? row.tags.map(tag => <span key={tag}>{tag}<button type="button" disabled={busy} aria-label={`移除 Tag ${tag}`} onClick={() => void save(row.tags.filter(name => name !== tag))}><X size={11} /></button></span>) : <p className="r-muted">尚无 Tag</p>}</div><div className="rd-tag-input"><Input role="combobox" aria-label="添加 Tag" aria-expanded={focused && suggestions.length > 0} aria-controls={`row-tags-${row.id}`} aria-activedescendant={choice >= 0 ? `row-tag-${row.id}-${choice}` : undefined} placeholder="输入 Tag，回车即建即贴…" value={query} disabled={busy} onFocus={() => setFocused(true)} onBlur={() => { setFocused(false); setChoice(-1); }} onChange={event => { setQuery(event.target.value); setChoice(-1); }} onKeyDown={event => {
    if (event.nativeEvent.isComposing) return;
    if (event.key === "ArrowDown") { event.preventDefault(); setChoice(value => Math.min(suggestions.length - 1, value + 1)); }
    if (event.key === "ArrowUp") { event.preventDefault(); setChoice(value => Math.max(-1, value - 1)); }
    if (event.key === "Enter") { event.preventDefault(); add(choice >= 0 ? suggestions[choice]?.name || query : query); }
    if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); setQuery(""); event.currentTarget.blur(); }
  }} />{focused && suggestions.length > 0 && <div id={`row-tags-${row.id}`} className="rd-tag-suggestions" role="listbox" aria-label="Tag 建议">{suggestions.map((tag, index) => <button id={`row-tag-${row.id}-${index}`} type="button" role="option" aria-selected={choice === index} key={tag.name} onMouseDown={event => event.preventDefault()} onClick={() => add(tag.name)}>{tag.name}</button>)}</div>}</div>{error && <p className="r-field-error" role="alert">保存失败：{error}</p>}</section>;
}
