import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Images, Palette, Layers, SlidersHorizontal, ScanLine, RefreshCw, Copy, ChevronDown, MousePointer2, Maximize2, ListFilter } from "lucide-react";
import type { RowRecord } from "../../../lib/api";
import { rowFileName, rowResolution } from "../../../lib/utils/row-display";
import { clearProgressiveImages } from "../../../lib/images/progressive-images";
import { thumbnails } from "../../ui/use-image";
import { Button, Tooltip } from "../../ui/controls";
import { Notices } from "../../ui/Notices";
import { WindowControls } from "../../ui/WindowControls";
import { useRevealWindow } from "../../ui/reveal-window";
import { notifyMainStateChanged, notifyToolboxLibraryChanged } from "../../../lib/windows/library-events";
import type { AfterRowsDeleted } from "../../state/row-actions";
import { DeleteRowsDialog } from "../DeleteRowsDialog";
import { notify } from "../../state/notices";
import { closeSideBySide, compareList, compareNavigation, loadMoreCompareList, moveCompareTarget, openSideBySide, pickCompareTarget, refreshCompare, resetCompare, selectCompareTab, setCompareSample, useCompare } from "../../state/compare";
import { ArtistText } from "../../ui/ArtistText";
import { focusMainWindow } from "../../../lib/windows/toolbox";
import { reportAgentCompareView } from "../../../lib/api/agent";
import { OriginalLightbox, PaneImage } from "./CompareImages";
import { SideBySide } from "./SideBySide";
import { CompareRowContextMenu } from "./CompareRowContextMenu";
import { Badges, CompareSection, ModelSection, type Reveal } from "./CompareResults";
import { compareFields, inspectionTab, type CompareField } from "../../../lib/api/compare-conditions";
import type { CompareWindowView } from "../../../lib/windows/compare";
import "./compare.css";

const tabs = [
  { key: "artists", label: "相同画师串", hint: "查看同一画师组合的创作", icon: Palette },
  { key: "vibeDiffStyle", label: "相同 VIBE", hint: "比较不同提示词的效果", icon: Layers },
  { key: "styleDiffVibe", label: "相同提示词", hint: "比较不同 VIBE 的效果", icon: ScanLine },
  { key: "models", label: "不同模型", hint: "比较相同提示词下的模型表现", icon: SlidersHorizontal },
] as const;
const AGENT_LISTED_LIMIT = 60;
const rowTitle = (row: RowRecord) => row.note?.trim() || rowFileName(row) || `第 ${row.sourceOrdinal} 张`;
const isField = (value: string | null): value is CompareField => value !== null && Object.hasOwn(compareFields, value);

/** The relation or custom comparison a newly opened window should start on (set by the backend). */
function viewFromQuery(params: URLSearchParams): CompareWindowView | undefined {
  const tab = params.get("tab");
  if (tab === "custom") {
    const observe = params.get("observe");
    if (!isField(observe)) return undefined;
    const allowed = params.get("allowed");
    return { custom: { observe, dedupe: params.get("dedupe") === "1", allowed: allowed === null ? null : allowed.split(",").filter(isField) } };
  }
  return tabs.some(item => item.key === tab) ? { tab: tab as CompareWindowView["tab"] } : undefined;
}

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
  const pendingPick = useRef<string | null>(null);
  const [original, setOriginal] = useState<RowRecord | null>(null);
  async function chooseTarget() {
    if (!state.sample) return;
    const requestId = crypto.randomUUID();
    pendingPick.current = requestId;
    try {
      await emitTo("main", "compare://pick-target", { sampleId: state.sample.row.id, requestId });
      await focusMainWindow();
    } catch (error) { notify(`无法打开主画廊：${String(error)}`, "error"); }
  }
  const results = useRef<HTMLElement>(null);
  const lastTarget = useRef<number | null>(null);
  const [reveal, setReveal] = useState<Reveal>(null);
  const [opensOnSample] = useState(() => Number(new URLSearchParams(window.location.search).get("row")) > 0);
  useRevealWindow(!opensOnSample || !!state.sample || !!state.sampleError);
  useEffect(() => {
    let disposed = false;
    const cleanups: (() => void)[] = [];
    const register = (promise: Promise<() => void>) => void promise.then(fn => { if (disposed) fn(); else cleanups.push(fn); }, error => { if (!disposed) notify(`窗口事件连接失败：${String(error)}`, "error"); });
    register(listen<number | { rowId: number; targetId?: number | null; view?: CompareWindowView | null }>("compare://set-sample", ({ payload }) => {
      if (disposed) return;
      pendingPick.current = null;
      if (typeof payload === "number") void setCompareSample(payload, true);
      else void setCompareSample(payload.rowId, true, payload.targetId ?? undefined, payload.view ?? undefined);
    }));
    register(listen<{ requestId: string; sampleId: number; row: RowRecord | null }>("compare://pick-finished", ({ payload }) => {
      if (disposed || pendingPick.current !== payload.requestId || useCompare.getState().rowId !== payload.sampleId) return;
      pendingPick.current = null;
      if (payload.row && payload.row.id !== payload.sampleId) openSideBySide(payload.row, true);
      const win = getCurrentWindow();
      void win.unminimize().then(() => win.setFocus()).catch(error => notify(`无法聚焦对比窗口：${String(error)}`, "error"));
    }));
    register(listen("artist-matching://changed", () => { if (!disposed && useCompare.getState().sample) void refreshCompare(); }));
    register(listen("main://library-reset", () => {
      if (disposed) return;
      resetCompare(); thumbnails.clear(); clearProgressiveImages();
      void getCurrentWindow().destroy().catch(error => notify(`关闭对比窗口失败：${String(error)}`, "error"));
    }));
    const params = new URLSearchParams(window.location.search);
    const initialRowId = Number(params.get("row"));
    if (initialRowId > 0) void setCompareSample(initialRowId, false, Number(params.get("target")) || undefined, viewFromQuery(params));
    return () => { disposed = true; cleanups.forEach(fn => fn()); };
  }, []);
  const listed = state.activeTab === "models" ? null : state[state.activeTab].items;
  const listedKey = listed?.slice(0, AGENT_LISTED_LIMIT).map(row => row.id).join(",") ?? "";
  const listedTotal = state.activeTab === "models" ? null : state[state.activeTab].loaded ? state[state.activeTab].total : null;
  const expandedKey = state.expandedGroup?.slice(0, AGENT_LISTED_LIMIT).join(",") ?? "";
  useEffect(() => {
    // Lets the library agent see what the user is comparing ("these two", "the A image", "this group").
    const tab = state.activeTab === "custom" ? "自定义对比" : tabs.find(item => item.key === state.activeTab)?.label ?? state.activeTab;
    void reportAgentCompareView(state.rowId === null ? null : {
      sampleRowId: state.rowId,
      targetRowId: state.target?.id ?? null,
      relation: state.target ? (state.manualTarget ? "手动选择的 B" : tab) : null,
      activeTab: tab,
      ...(state.activeTab === "custom" && { customConditions: state.options }),
      listedTotal,
      listedRowIds: listedKey ? listedKey.split(",").map(Number) : [],
      ...(expandedKey && { expandedGroupRowIds: expandedKey.split(",").map(Number), expandedGroupSize: state.expandedGroup?.length }),
    }).catch(() => {});
  }, [state.rowId, state.target?.id, state.manualTarget, state.activeTab, state.options, listedKey, listedTotal, expandedKey, state.expandedGroup?.length]);
  useEffect(() => { results.current?.scrollTo({ top: 0 }); }, [state.rowId, state.activeTab, state.options]);
  useLayoutEffect(() => {
    // Back from the side-by-side view, bring the last B into view in the results (or reopen its group).
    if (state.target) { lastTarget.current = state.target.id; return; }
    if (lastTarget.current === null) return;
    const group = useCompare.getState().expandedGroup;
    setReveal(group ? { rowId: group[0], memberId: lastTarget.current } : { rowId: lastTarget.current });
  }, [state.target]);
  const sample = state.sample;
  const navigation = compareNavigation(state);
  const list = compareList(state);
  const relation = state.manualTarget ? "自选图片对比" : state.activeTab === "custom" ? "自定义对比" : tabs.find(tab => tab.key === state.activeTab)!.label;
  const tabButton = (key: (typeof tabs)[number]["key"] | "custom", label: string, hint: string, Icon: typeof Palette, onClick: () => void) =>
    <button key={key} type="button" className={state.activeTab === key ? "active" : ""} aria-pressed={state.activeTab === key} title={hint} onClick={onClick}><Icon size={16} /><span>{label}</span></button>;
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
                    <div className="sidebar-label"><span className="mark">A</span><strong>参考样本</strong><span>#{sample.row.id}</span><button className="icon-button" aria-label="刷新样本与关联结果" title="刷新样本与关联结果" onClick={() => void refreshCompare()}><RefreshCw size={14} /></button></div>
                    <SampleCard row={sample.row} onOpen={() => setOriginal(sample.row)} />
                    <nav className="relation-nav" aria-label="对比关系">
                      {tabs.map(tab => tabButton(tab.key, tab.label, tab.hint, tab.icon, () => void selectCompareTab(tab.key)))}
                      {tabButton("custom", "自定义对比", "自己选要比较哪一项、哪些条件要和 A 一致", ListFilter, () => void selectCompareTab("custom"))}
                      <button type="button" className="relation-manual" title="到主画廊挑一张图作为 B" onClick={() => void chooseTarget()}><MousePointer2 size={16} /><span>手动选择 B</span></button>
                    </nav>
                    <SamplePrompts key={sample.row.id} row={sample.row} />
                  </aside>
                  <main className="results" ref={results}>{state.activeTab === "models" ? <ModelSection scrollRef={results} reveal={reveal} key={`${sample.row.id}-models`} /> : <CompareSection scrollRef={results} reveal={reveal} key={`${sample.row.id}-${state.activeTab}`} section={state.activeTab} />}</main>
                </div>
                {state.target && <SideBySide initialTab={state.activeTab === "custom" && !state.manualTarget ? inspectionTab(state.options.observe) : undefined} key={sample.row.id} sample={sample.row} target={state.target} relation={relation}
                  navigation={navigation} film={list && { rows: list.rows, more: list.more }} onMove={moveCompareTarget} onPick={pickCompareTarget} onLoadMore={loadMoreCompareList} onBack={closeSideBySide} onChooseTarget={chooseTarget} />}
              </>}
      </div>
    </div>
    {original && <OriginalLightbox key={original.id} row={original} onClose={() => setOriginal(null)} />}
    <DeleteRowsDialog afterDelete={afterCompareDelete} /><Notices />
  </Tooltip.Provider>;
}

async function copy(text: string) { try { await navigator.clipboard.writeText(text); notify("已复制到剪贴板。"); } catch { notify("复制失败，请重试。", "error"); } }

function SampleCard({ row, onOpen }: { row: RowRecord; onOpen: () => void }) {
  const hasImage = Boolean(row.imagePath?.trim() || row.storedImagePath?.trim());
  return <CompareRowContextMenu row={row}><section className="rc-sample">
    <button type="button" className="sample-media" disabled={!hasImage} onClick={onOpen} aria-label="查看 A 的原图" title={hasImage ? "查看原图" : undefined}>
      <PaneImage row={row} tier="gallery" />
      <span className="card-badges"><Badges row={row} /></span>
      {hasImage && <span className="original-hint"><Maximize2 size={13} />原图</span>}
    </button>
    <h2 title={row.imagePath || undefined}>{rowTitle(row)}</h2>
    <p className="facts">{[rowResolution(row) || "尺寸未知", row.time?.split(" ")[0] || "时间未知"].join(" · ")}</p>
    {row.artists?.trim() && <div className="artists"><span title={row.artists.trim()}><ArtistText text={row.artists.trim()} /></span><button className="icon-button" aria-label="复制画师串" title="复制画师串" onClick={() => void copy(row.artists!.trim())}><Copy size={13} /></button></div>}
  </section></CompareRowContextMenu>;
}

function SamplePrompts({ row }: { row: RowRecord }) {
  const [open, setOpen] = useState(false);
  const fields = [{ label: "正向提示词", value: row.positivePrompt }, { label: "角色提示词", value: row.characterPrompt }, { label: "负向提示词", value: row.negativePrompt }];
  return <section className="sample-prompts"><button className="prompt-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>样本提示词<ChevronDown size={14} style={{ transform: open ? "rotate(180deg)" : undefined }} /></button>
    {open && <div className="prompt-list">{fields.map(field => <div key={field.label}><header><span>{field.label}</span>{field.value && <button onClick={() => void copy(field.value!)}>复制</button>}</header><pre>{field.value || "（空）"}</pre></div>)}</div>}
  </section>;
}
