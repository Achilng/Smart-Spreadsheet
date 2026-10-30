import { useEffect, useRef, useState } from "react";
import { DetailPrompt } from "./DetailPrompt";
import { markArtistRepresentative } from "../state/artist-representatives";
import { Check, Copy, FolderOpen, ImageOff, Maximize2, Bookmark } from "lucide-react";
import { type RowRecord } from "../../lib/api";
import { rowFileName, rowResolution } from "../../lib/utils/row-display";
import { modelVersionBadge } from "../../lib/utils/model-version";
import { errorText } from "../../lib/utils/format";
import { vibeStatuses } from "../../lib/images/vibe-statuses";
import { useImageCache } from "../state/image-cache";
import { notify } from "../state/notices";
import { useTasks } from "../state/tasks";
import { showRowInExplorer } from "../state/row-actions";
import { beginFileDrag } from "../state/file-drag";
import { Button, Hint } from "../ui/controls";
import { FieldEditor } from "../ui/FieldEditor";
import { useProgressiveImage } from "../ui/use-image";
import { ImageLightbox } from "../ui/ImageLightbox";
import "../ui/detail-images.css";
import "./detail-organization.css";
import { DetailTags, DetailOrganization, DetailFold } from "./DetailOrganization";

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
  return <><DetailTags row={row} /><div className="r-detail-scroll rd-contents">
    <div className="rd-preview"><button className="rd-preview-button" type="button" aria-label={`放大预览：${rowFileName(row) || `第 ${row.sourceOrdinal} 行图片`}`} disabled={!hasImage} onMouseDown={event => { if (hasImage) beginFileDrag(event.nativeEvent, row.id, () => { drag.current = true; }, () => { setTimeout(() => { drag.current = false; }, 0); }); }} onClick={() => { if (!drag.current) setLightbox(true); }}>
      {image.url && decoded ? <img src={image.url} alt={rowFileName(row) || "图片预览"} draggable={false} onError={() => setDecoded(false)} /> : <span className="rd-preview-empty">{!hasImage ? <><ImageOff size={22} />无图片</> : image.error || !decoded ? <><ImageOff size={22} />图片不可用</> : "正在加载图片…"}</span>}
      <span className="rd-preview-badges">{badge && <span className={`version-badge ${badge.className}`} title={`作画模型：${row.generationModel}`}>{badge.label}</span>}{Boolean(vibeCount) && <span className="vibe-badge" title={`原图元数据包含 ${vibeCount} 个 VIBE 引用，拖到 NovelAI 可一并导入`}>VIBE ×{vibeCount}</span>}</span>{hasImage && <span className="rd-preview-open"><Maximize2 size={13} />查看原图</span>}
    </button><button type="button" className="rd-representative" data-active={row.artistRepresentative} aria-pressed={row.artistRepresentative} aria-label={row.artistRepresentative ? "取消代表图" : "设为代表图"} title={row.artists?.trim() ? "按画师串去重时优先显示这张图片" : "此图片暂无画师串"} disabled={busy || !row.artists?.trim()} onClick={() => void markArtistRepresentative(row, !row.artistRepresentative)}><Bookmark size={13} />{row.artistRepresentative ? "代表图" : "设为代表图"}</button>{image.error && <div className="rd-preview-retry"><span>{image.url ? "高清预览加载失败" : "预览加载失败"}</span><Button size="sm" variant="ghost" onClick={image.retry}>重新加载</Button></div>}</div>
    <DetailOrganization row={row} />
    <DetailPrompt row={row} />
    {([["角色提示词", "characterPrompt"], ["负向提示词", "negativePrompt"]] as const).map(([label, field]) => <DetailFold key={field} name={field} label={label} summary={row[field] || "未填写"}><FieldEditor row={row} label={label} field={field} /></DetailFold>)}
    <DetailFold name="metadata" label="更多信息" summary={row.metadataFailed ? "生成信息未完整解析" : "生成参数 · 文件位置"}><section><h4>生成信息</h4><dl className="r-parameters">{[
      ["模型", row.generationModel], ["采样器", row.generationSampler], ["步数", row.generationSteps], ["种子", row.generationSeed], ["CFG", row.generationScale], ["CFG Rescale", row.generationCfgRescale], ["噪声调度", row.generationNoiseSchedule], ["尺寸", rowResolution(row)], ["时间", row.time],
    ].filter(([, value]) => value !== null && value !== undefined && value !== "").map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>{row.metadataFailed && <p className="rd-metadata-warning">这张图片的生成信息未能完整解析。</p>}</section>
    {([{ label: "图片文件夹", value: row.imageFolder }, { label: "图片路径", value: row.imagePath }] as const).map(field => <section key={field.label}><div className="r-section-heading"><h4>{field.label}</h4><div className="r-field-actions">{field.label === "图片路径" && hasImage && <Hint text="在资源管理器中定位"><Button size="icon" variant="ghost" aria-label="在资源管理器中定位" onClick={() => void showRowInExplorer(row)}><FolderOpen size={13} /></Button></Hint>}<Hint text={`复制${field.label}`}><Button size="icon" variant="ghost" disabled={!field.value} aria-label={`复制${field.label}`} onClick={() => void copy(field.label, field.value!)}>{copied === field.label ? <Check size={13} /> : <Copy size={13} />}</Button></Hint></div></div><p className={field.value ? "r-prompt" : "r-muted"}>{field.value || "—"}</p></section>)}
    </DetailFold>
  </div>{lightbox && <ImageLightbox row={row} onClose={() => setLightbox(false)} />}</>;
}
