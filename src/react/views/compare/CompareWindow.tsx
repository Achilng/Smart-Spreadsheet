import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Images, Palette, Layers, SlidersHorizontal, ScanLine, RefreshCw, Copy, ChevronDown, ChevronLeft, ChevronRight, ArrowUpRight } from "lucide-react";
import type { RowRecord } from "../../../lib/api";
import { modelVersionBadge } from "../../../lib/utils/model-version";
import { rowFileName, rowResolution } from "../../../lib/utils/row-display";
import { formatCount } from "../../../lib/utils/format";
import { clearProgressiveImages } from "../../../lib/images/progressive-images";
import { thumbnails } from "../../ui/use-image";
import { Button, Tooltip } from "../../ui/controls";
import { Notices } from "../../ui/Notices";
import { WindowControls } from "../../ui/WindowControls";
import { notifyMainStateChanged, notifyToolboxLibraryChanged } from "../../../lib/windows/library-events";
import type { AfterRowsDeleted } from "../../state/row-actions";
import { DeleteRowsDialog } from "../DeleteRowsDialog";
import { notify } from "../../state/notices";
import { closeSideBySide, compareModelGroups, compareNavigation, loadCompareModels, loadCompareSectionPage, moveCompareTarget, openSideBySide, refreshCompare, resetCompare, selectCompareModel, selectCompareTab, setCompareModelOffset, setCompareSample, useCompare, type CompareSectionKey } from "../../state/compare";
import { ArtistText } from "../../ui/ArtistText";
import { PaneImage } from "./CompareImages";
import { SideBySide } from "./SideBySide";
import { CompareRowContextMenu } from "./CompareRowContextMenu";
import "./compare.css";

const tabs = [
  { key: "artists", label: "相同画师串", hint: "查看同一画师组合的创作", icon: Palette },
  { key: "vibeDiffStyle", label: "相同 VIBE", hint: "比较不同提示词的效果", icon: Layers },
  { key: "styleDiffVibe", label: "相同提示词", hint: "比较不同 VIBE 的效果", icon: ScanLine },
  { key: "models", label: "不同模型", hint: "比较相同提示词下的模型表现", icon: SlidersHorizontal },
] as const;
const rowTitle = (row: RowRecord) => row.note?.trim() || rowFileName(row) || `第 ${row.sourceOrdinal} 张`;

const afterCompareDelete: AfterRowsDeleted = async (_result, selection) => {
  thumbnails.clear(); clearProgressiveImages();
  await notifyMainStateChanged("libraryEdited");
  notifyToolboxLibraryChanged("main");
  const sampleId = useCompare.getState().rowId;
  if (sampleId !== null && selection.kind === "explicit" && selection.rowIds.includes(sampleId)) resetCompare();
  else await refreshCompare();
};

export function CompareWindow() {
  const state = useCompare();
  const results = useRef<HTMLElement>(null);
  const lastTarget = useRef<number | null>(null);
  useEffect(() => {
    let disposed = false;
    const cleanups: (() => void)[] = [];
    const register = (promise: Promise<() => void>) => void promise.then(fn => { if (disposed) fn(); else cleanups.push(fn); }, error => { if (!disposed) notify(`窗口事件连接失败：${String(error)}`, "error"); });
    register(listen<number>("compare://set-sample", event => { if (!disposed) void setCompareSample(event.payload); }));
    register(listen("artist-matching://changed", () => { if (!disposed && useCompare.getState().sample) void refreshCompare(); }));
    register(listen("main://library-reset", () => {
      if (disposed) return;
      resetCompare(); thumbnails.clear(); clearProgressiveImages();
      void getCurrentWindow().destroy().catch(error => notify(`关闭对比窗口失败：${String(error)}`, "error"));
    }));
    const initialRowId = Number(new URLSearchParams(window.location.search).get("row"));
    if (initialRowId > 0) void setCompareSample(initialRowId);
    return () => { disposed = true; cleanups.forEach(fn => fn()); };
  }, []);
  useEffect(() => { results.current?.scrollTo({ top: 0 }); }, [state.rowId, state.activeTab, state.artists.offset, state.vibeDiffStyle.offset, state.styleDiffVibe.offset, state.modelTier, state.modelOffset]);
  useLayoutEffect(() => {
    if (state.target) { lastTarget.current = state.target.id; return; }
    const button = results.current?.querySelector<HTMLButtonElement>(`[data-compare-id="${lastTarget.current}"]`);
    button?.focus({ preventScroll: true });
    button?.scrollIntoView({ block: "nearest" });
  }, [state.target]);
  const sample = state.sample;
  const navigation = compareNavigation(state);
  const relation = tabs.find(tab => tab.key === state.activeTab)!.label;
  return <Tooltip.Provider delayDuration={350}>
    <div className="rc-window" onContextMenu={event => { if (!(event.target instanceof Element && event.target.closest("input, textarea, [contenteditable='true']"))) event.preventDefault(); }}>
      <header className="titlebar" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region><Images size={17} /><strong data-tauri-drag-region>图片对比</strong></div>
        <WindowControls />
      </header>
      <div className="compare-body">
        {state.sampleLoading ? <div className="state-page" role="status"><span className="rc-image-loading" /><h1>正在准备对比</h1><p>读取样本与关联图片…</p></div>
          : state.sampleError ? <div className="state-page"><Images size={32} /><h1>样本加载失败</h1><p role="alert">{state.sampleError}</p><Button onClick={() => void refreshCompare()}>重新加载</Button></div>
            : !sample ? <div className="state-page"><Images size={36} /><h1>从一张图片开始</h1><p>在主窗口右键图片，选择“对比”，探索画师、提示词与模型之间的关联。</p></div>
              : <>
                <div className="workspace" hidden={Boolean(state.target)}>
                  <aside className="sidebar" aria-label="对比样本与关系">
                    <div className="sidebar-label"><strong>参考样本</strong><span>#{sample.row.id}</span><button className="icon-button" aria-label="刷新样本与关联结果" title="刷新样本与关联结果" onClick={() => void refreshCompare()}><RefreshCw size={14} /></button></div>
                    <SampleCard key={sample.row.id} row={sample.row} />
                    <nav className="relation-nav" aria-label="对比关系">{tabs.map(tab => <button key={tab.key} type="button" className={state.activeTab === tab.key ? "active" : ""} aria-pressed={state.activeTab === tab.key} onClick={() => void selectCompareTab(tab.key)}>
                      <tab.icon size={17} /><span className="nav-copy"><strong>{tab.label}</strong><small>{tab.hint}</small></span><ChevronRight className="nav-arrow" size={14} />
                    </button>)}</nav>
                    <SamplePrompts key={sample.row.id} row={sample.row} />
                    <p className="sidebar-tip">点选右侧图片，与样本并排对照。</p>
                  </aside>
                  <main className="results" ref={results}>{state.activeTab === "models" ? <ModelSection key={`${sample.row.id}-models`} /> : <CompareSection key={`${sample.row.id}-${state.activeTab}`} section={state.activeTab} />}</main>
                </div>
                {state.target && <SideBySide key={sample.row.id} sample={sample.row} target={state.target} relation={relation} navigation={navigation} onMove={moveCompareTarget} onBack={closeSideBySide} />}
              </>}
      </div>
    </div><DeleteRowsDialog afterDelete={afterCompareDelete} /><Notices />
  </Tooltip.Provider>;
}

function Badges({ row }: { row: RowRecord }) {
  const badge = modelVersionBadge(row.generationModel);
  return <>{badge && <span className={`version-badge ${badge.className}`}>{badge.label}</span>}{Boolean(row.vibeReferenceCount) && <span className="vibe-badge">VIBE ×{row.vibeReferenceCount}</span>}</>;
}
async function copy(text: string) { try { await navigator.clipboard.writeText(text); notify("已复制到剪贴板。"); } catch { notify("复制失败，请重试。", "error"); } }

function SampleCard({ row }: { row: RowRecord }) {
  return <CompareRowContextMenu row={row}><section className="rc-sample">
    <div className="sample-overview"><div className="sample-media"><PaneImage row={row} tier="gallery" /></div>
      <div className="sample-meta"><h2 title={row.imagePath || undefined}>{rowTitle(row)}</h2><p className="facts">{rowResolution(row) || "尺寸未知"}</p><p className="facts">{row.time?.split(" ")[0] || "时间未知"}</p><div className="sample-badges"><Badges row={row} /></div></div>
    </div>
    {row.artists?.trim() && <div className="artists"><span title={row.artists.trim()}><ArtistText text={row.artists.trim()} /></span><button className="icon-button" aria-label="复制画师串" onClick={() => void copy(row.artists!.trim())}><Copy size={13} /></button></div>}
  </section></CompareRowContextMenu>;
}

function SamplePrompts({ row }: { row: RowRecord }) {
  const [open, setOpen] = useState(false);
  const fields = [{ label: "正向提示词", value: row.positivePrompt }, { label: "角色提示词", value: row.characterPrompt }, { label: "负向提示词", value: row.negativePrompt }];
  return <section className="sample-prompts"><button className="prompt-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>样本提示词<ChevronDown size={14} style={{ transform: open ? "rotate(180deg)" : undefined }} /></button>
    {open && <div className="prompt-list">{fields.map(field => <div key={field.label}><header><span>{field.label}</span>{field.value && <button onClick={() => void copy(field.value!)}>复制</button>}</header><pre>{field.value || "（空）"}</pre></div>)}</div>}
  </section>;
}

function CompareCard({ row }: { row: RowRecord }) {
  return <CompareRowContextMenu row={row}><article className="rc-card">
    <div className="thumb"><PaneImage row={row} tier="thumbnail" lazy /><div className="card-badges"><Badges row={row} /></div><button type="button" className="card-open" data-compare-id={row.id} aria-label={`对比这张：${rowTitle(row)}`} title={[rowFileName(row), rowResolution(row), row.imagePath].filter(Boolean).join("\n")} onClick={() => openSideBySide(row)}><span className="open-hint"><ArrowUpRight size={14} />对比这张</span></button></div>
    <div className="card-info"><span className="card-label" title={rowTitle(row)}>{rowTitle(row)}</span><span className="card-sub"><span>{rowResolution(row) || "尺寸未知"}</span><span>#{row.id}</span></span></div>
  </article></CompareRowContextMenu>;
}

function Pagination({ offset, count, total, loading, onPrevious, onNext }: { offset: number; count: number; total: number; loading?: boolean; onPrevious: () => void; onNext: () => void }) {
  return <nav className="pagination" aria-label="结果分页"><span role="status">{loading ? "正在加载…" : `${formatCount(offset + 1)}–${formatCount(offset + count)} / ${formatCount(total)}`}</span><button className="icon-button" aria-label="上一页" disabled={loading || offset === 0} onClick={onPrevious}><ChevronLeft size={16} /></button><button className="icon-button" aria-label="下一页" disabled={loading || offset + count >= total} onClick={onNext}><ChevronRight size={16} /></button></nav>;
}

function CompareSection({ section }: { section: CompareSectionKey }) {
  const state = useCompare(s => s[section]);
  const sample = useCompare(s => s.sample)!;
  const title = section === "artists" ? "相同画师串" : section === "vibeDiffStyle" ? "相同 VIBE · 不同提示词" : "相同提示词 · 不同 VIBE";
  const unavailable = section === "artists" ? !sample.row.artists?.trim() : section === "vibeDiffStyle" ? !sample.hasVibeSignature : !sample.hasStyleSignature;
  const empty = section === "artists" ? "样本还没有画师串，试试其他对比关系。" : section === "vibeDiffStyle" ? sample.vibeSignatureUnreadable ? "样本原图不可读，无法读取 VIBE 引用。" : "样本没有引用 VIBE，试试其他对比关系。" : "样本没有可比较的提示词，试试其他对比关系。";
  return <section className="rc-section" aria-busy={state.loading}>
    <header className="section-head"><div className="section-copy"><h1>{title}{state.loaded && <span className="section-count">{formatCount(state.total)} 张</span>}</h1><p>{section === "artists" ? "完整画师串一致，比较同一组合的不同创作。" : section === "vibeDiffStyle" ? "保持 VIBE 引用不变，观察正向提示词带来的变化。" : "正向提示词一致（忽略官方质量词），观察 VIBE 引用带来的变化。"}</p></div>{state.items.length > 0 && <Pagination offset={state.offset} count={state.items.length} total={state.total} loading={state.loading} onPrevious={() => void loadCompareSectionPage(section, false, -1)} onNext={() => void loadCompareSectionPage(section)} />}</header>
    {state.error && <div className="error" role="alert"><span>{state.error}</span><Button onClick={() => void loadCompareSectionPage(section, true)}>重新加载</Button></div>}
    {state.items.length ? <div className={`section-grid ${state.loading ? "loading" : ""}`}>{state.items.map(row => <CompareCard key={row.id} row={row} />)}</div> : state.loading ? <div className="skeleton-grid" role="status" aria-label="正在加载关联图片">{Array.from({ length: 6 }, (_, i) => <div className="skeleton" key={i} />)}</div> : !state.error && <div className="empty"><Images size={28} /><h2>{unavailable ? "缺少对比信息" : "暂时没有匹配图片"}</h2><p>{unavailable ? empty : "资料库中还没有符合这个关系的图片，可以切换其他关系继续探索。"}</p></div>}
  </section>;
}

function ModelSection() {
  const { models, modelsLoading, modelsError, sample, modelTier, modelOffset } = useCompare();
  const groups = useMemo(() => compareModelGroups(models, sample?.row), [models, sample?.row]);
  const active = groups.find(group => group.tier === modelTier) || groups[0];
  const rows = active?.rows.slice(modelOffset, modelOffset + 24) || [];
  return <section className="rc-section" aria-busy={modelsLoading}>
    <header className="section-head"><div className="section-copy"><h1>相同提示词 · 不同模型{!modelsLoading && <span className="section-count">{formatCount(groups.reduce((sum, group) => sum + group.rows.length, 0))} 张</span>}</h1><p>保持正向提示词一致，比较不同作画模型的表现。</p></div>{active && <Pagination offset={modelOffset} count={rows.length} total={active.rows.length} loading={modelsLoading} onPrevious={() => setCompareModelOffset(modelOffset - 24)} onNext={() => setCompareModelOffset(modelOffset + 24)} />}</header>
    {models.truncated && <div className="truncate-note">仅比较最新 500 张同画风图片。其他模型的结果也可能在更早的图片中。</div>}
    {modelsError ? <div className="error" role="alert">{modelsError}<Button onClick={() => void loadCompareModels()}>重新加载</Button></div> : modelsLoading ? <div className="empty" role="status">正在整理模型结果…</div> : active ? <><nav className="model-options" aria-label="模型分组">{groups.map(group => <button key={group.tier} className={active.tier === group.tier ? "active" : ""} aria-pressed={active.tier === group.tier} onClick={() => selectCompareModel(group.tier)}>{group.title}<span>{group.rows.length}</span></button>)}</nav><div className="section-grid">{rows.map(row => <CompareCard key={row.id} row={row} />)}</div></> : <div className="empty"><Images size={28} /><h2>{!sample?.hasStyleSignature ? "缺少对比信息" : "暂时没有其他模型的图片"}</h2><p>{!sample?.hasStyleSignature ? "样本没有可比较的提示词。" : models.truncated ? "最新 500 张同画风图片中，没有其他模型的结果。" : models.totalCount ? "找到的相同画风图片都与样本使用同一模型。" : "资料库中还没有符合条件的图片。"}</p></div>}
  </section>;
}
