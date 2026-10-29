import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { ArrowLeft, ChevronLeft, ChevronRight, Crosshair, Maximize2 } from "lucide-react";
import type { RowRecord } from "../../../lib/api";
import { diffPromptField, type PromptToken } from "../../../lib/utils/prompt-diff";
import { modelVersionBadge } from "../../../lib/utils/model-version";
import { rowFileName, rowResolution } from "../../../lib/utils/row-display";
import { formatCount } from "../../../lib/utils/format";
import { openRowInMainWindow } from "../../../lib/windows/toolbox";
import type { compareNavigation } from "../../state/compare";
import { notify } from "../../state/notices";
import { PaneImage, OriginalLightbox } from "./CompareImages";

function Tokens({ items, quality }: { items: PromptToken[]; quality: boolean }) {
  return <ul className="tokens">{items.map((token, index) => <li key={index} className={`token ${quality && token.isQuality ? "quality" : ""}`}>{token.display}</li>)}</ul>;
}
const value = (input: string | number | null | undefined) => input == null || input === "" ? "未记录" : String(input);
type Navigation = ReturnType<typeof compareNavigation>;
interface Props { sample: RowRecord; target: RowRecord; relation: string; navigation: Navigation; onMove: (direction: 1 | -1) => Promise<void>; onBack: () => void }

export function SideBySide({ sample, target, relation, navigation, onMove, onBack }: Props) {
  const [tab, setTab] = useState<"positive" | "character" | "params">("positive");
  const [showShared, setShowShared] = useState(false);
  const [onlyChangedParams, setOnlyChangedParams] = useState(true);
  const [originalRow, setOriginalRow] = useState<RowRecord | null>(null);
  const [inspectorWidth, setInspectorWidth] = useState(360);
  const [maximumWidth, setMaximumWidth] = useState(600);
  const [dragging, setDragging] = useState(false);
  const [lastDirection, setLastDirection] = useState<1 | -1>(1);
  const layout = useRef<HTMLDivElement>(null);
  const backButton = useRef<HTMLButtonElement>(null);
  useLayoutEffect(() => { backButton.current?.focus({ preventScroll: true }); }, []);
  const positive = useMemo(() => diffPromptField(sample.positivePrompt, target.positivePrompt), [sample.positivePrompt, target.positivePrompt]);
  const character = useMemo(() => diffPromptField(sample.characterPrompt, target.characterPrompt), [sample.characterPrompt, target.characterPrompt]);
  const diff = tab === "character" ? character : positive;
  const changes = diff.onlyLeft.length + diff.onlyRight.length;
  const params = [
    ["模型", value(sample.generationModel), value(target.generationModel), "生成这张图片使用的模型"],
    ["采样器", value(sample.generationSampler), value(target.generationSampler), "Sampler"],
    ["步数", value(sample.generationSteps), value(target.generationSteps), "Steps"],
    ["种子", value(sample.generationSeed), value(target.generationSeed), "Seed"],
    ["提示词引导", value(sample.generationScale), value(target.generationScale), "Guidance / CFG Scale"],
    ["引导重缩放", value(sample.generationCfgRescale), value(target.generationCfgRescale), "CFG Rescale"],
    ["噪声调度", value(sample.generationNoiseSchedule), value(target.generationNoiseSchedule), "Noise Schedule"],
    ["尺寸", value(rowResolution(sample)), value(rowResolution(target)), "宽度 × 高度，单位为像素"],
  ].map(([label, a, b, help]) => ({ label, a, b, help, missing: a === "未记录" || b === "未记录", changed: a !== "未记录" && b !== "未记录" && a !== b }));
  const changedParams = params.filter(row => row.changed).length;
  const missingParams = params.filter(row => row.missing).length;
  const visibleParams = params.filter(row => !onlyChangedParams || row.changed || row.missing);

  function move(direction: 1 | -1) { setLastDirection(direction); void onMove(direction); }
  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      if (event.defaultPrevented || originalRow || event.ctrlKey || event.metaKey || event.altKey) return;
      const element = event.target instanceof Element ? event.target : null;
      if (element?.closest("input, textarea, select, [contenteditable='true'], [role='separator'], [role='dialog']")) return;
      if (event.key === "Escape") { event.preventDefault(); onBack(); }
      if ((event.key === "ArrowLeft" || event.key === "ArrowRight") && !window.getSelection()?.toString()) {
        event.preventDefault();
        const direction = event.key === "ArrowLeft" ? -1 : 1;
        setLastDirection(direction);
        void onMove(direction);
      }
    };
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, [originalRow, onBack, onMove]);
  useEffect(() => {
    const element = layout.current;
    if (!element) return;
    const observer = new ResizeObserver(() => {
      const maximum = Math.max(300, Math.min(600, Math.floor(element.clientWidth / 2)));
      setMaximumWidth(maximum);
      setInspectorWidth(current => Math.min(current, maximum));
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  async function locate(rowId: number) {
    try { await openRowInMainWindow(rowId); }
    catch (error) { notify(`无法在主画廊定位：${String(error)}`, "error"); }
  }
  return <>
    <div className="rc-side">
      <header className="compare-toolbar">
        <button ref={backButton} className="back" onClick={onBack} title="返回结果（Esc）"><ArrowLeft size={16} />返回结果</button>
        <div className="comparison-context"><strong>{relation}</strong><span>{navigation.group ?? "固定样本 A · 切换目标 B"}</span></div>
        <nav className="target-navigation" aria-label="切换对比目标">
          <span className="target-position" role="status" aria-live="polite">{navigation.loading ? "正在读取…" : `${formatCount(navigation.position)} / ${formatCount(navigation.total)}`}</span>
          <button className="icon-button" disabled={navigation.loading || !navigation.previous} aria-label="上一张目标" title="上一张目标（←）" onClick={() => move(-1)}><ChevronLeft size={18} /></button>
          <button className="icon-button" disabled={navigation.loading || !navigation.next} aria-label="下一张目标" title="下一张目标（→）" onClick={() => move(1)}><ChevronRight size={18} /></button>
        </nav>
      </header>
      {navigation.error && <div className="navigation-error" role="alert"><span>读取目标失败：{navigation.error}</span><button disabled={navigation.loading} onClick={() => move(lastDirection)}>重新加载</button></div>}
      <div ref={layout} className={`comparison-layout ${dragging ? "resizing" : ""}`} style={{ "--inspector-width": `${inspectorWidth}px` } as CSSProperties}>
        <main className="image-stage" aria-label="图片并排对照">
          <div className="panes">{[{ label: "参考样本", mark: "A", row: sample }, { label: "对比目标", mark: "B", row: target }].map(pane => {
            const hasImage = Boolean(pane.row.imagePath?.trim() || pane.row.storedImagePath?.trim());
            const badge = modelVersionBadge(pane.row.generationModel);
            return <figure className={`pane ${pane.mark === "B" ? "target" : ""}`} key={pane.mark}>
              <figcaption><span className="mark">{pane.mark}</span><strong>{pane.label}</strong><span className="resolution">{rowResolution(pane.row) || "尺寸未知"}</span></figcaption>
              <div className="pane-media"><PaneImage key={pane.row.id} row={pane.row} />{badge && <span className={`version-badge ${badge.className}`}>{badge.label}</span>}<button className="image-open" disabled={!hasImage} onClick={() => setOriginalRow(pane.row)} aria-label={`查看${pane.mark === "A" ? "样本" : "目标"}原图`}>{hasImage && <span className="original-hint"><Maximize2 size={13} />查看原图</span>}</button></div>
              <div className="pane-footer"><p title={pane.row.imagePath || undefined}>{pane.row.note?.trim() || rowFileName(pane.row) || `#${pane.row.id}`}</p><button onClick={() => void locate(pane.row.id)} title="在主画廊定位" aria-label={`在主画廊定位${pane.mark === "A" ? "样本" : "目标"}`}><Crosshair size={13} />定位</button></div>
            </figure>;
          })}</div>
          <footer className="stage-note"><span>点击查看原图 · 保持原始比例</span><span>← → 切换目标<span className="shortcut-divider">·</span>Esc 返回</span></footer>
        </main>
        <div className="inspector-resize" role="separator" tabIndex={0} aria-label="调整差异面板宽度" aria-orientation="vertical" aria-controls="compare-inspector" aria-valuenow={inspectorWidth} aria-valuemin={300} aria-valuemax={maximumWidth} title="拖动调整宽度；聚焦后使用左右键"
          onPointerDown={event => { if (event.button !== 0) return; event.currentTarget.setPointerCapture(event.pointerId); setDragging(true); }}
          onPointerMove={event => { if (!event.currentTarget.hasPointerCapture(event.pointerId) || !layout.current) return; setInspectorWidth(Math.max(300, Math.min(maximumWidth, Math.round(layout.current.getBoundingClientRect().right - event.clientX)))); }}
          onPointerUp={event => { if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); setDragging(false); }}
          onLostPointerCapture={() => setDragging(false)}
          onKeyDown={event => { if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return; event.preventDefault(); event.stopPropagation(); setInspectorWidth(current => event.key === "Home" ? 300 : event.key === "End" ? maximumWidth : Math.max(300, Math.min(maximumWidth, current + (event.key === "ArrowLeft" ? 24 : -24)))); }} />
        <aside id="compare-inspector" className="inspector" aria-label="图片差异">
          <header className="inspector-head"><h2>差异检查</h2><span>A / B</span></header>
          <nav className="diff-tabs" aria-label="差异类别">{([['positive', '正向提示词'], ['character', '角色提示词'], ['params', '生成参数']] as const).map(([key, label]) => <button key={key} className={tab === key ? "active" : ""} aria-pressed={tab === key} onClick={() => setTab(key)}>{label}{key === "params" && changedParams > 0 && <span>{changedParams}</span>}</button>)}</nav>
          <div className="inspector-scroll">{tab === "params" ? <>
            <div className="diff-summary"><div><strong>{changedParams} 项参数不同</strong>{missingParams > 0 && <small>{missingParams} 项缺少记录</small>}</div><button className="filter-toggle" onClick={() => setOnlyChangedParams(!onlyChangedParams)}>{onlyChangedParams ? "显示全部" : "只看不同"}</button></div>
            {visibleParams.length ? <table className="parameter-table"><thead><tr><th scope="col">参数</th><th scope="col">A 样本</th><th scope="col">B 目标</th></tr></thead><tbody>{visibleParams.map(row => <tr key={row.label} className={row.changed ? "changed" : row.missing ? "missing" : ""}><th scope="row" title={row.help}>{row.label}{row.changed ? <small>不同</small> : row.missing ? <small>缺少记录</small> : null}</th><td data-side="A">{row.a}</td><td data-side="B">{row.b}</td></tr>)}</tbody></table> : <p className="same-note">两张图片的生成参数完全一致。</p>}
            {missingParams > 0 && <p className="quality-note">未记录的参数无法判断是否一致，不计入“不同”数量。</p>}
          </> : <>
            <div className="diff-summary"><strong>{changes ? `${changes} 项提示词不同` : "提示词一致"}</strong><span>{diff.shared.length} 项共有</span></div>
            {changes === 0 && <p className="same-note">{diff.shared.length ? "两侧提示词项一致，可以继续比较生成参数。" : `两侧${tab === "character" ? "角色" : "正向"}提示词都为空。`}</p>}
            <div className="unique-columns"><section className="unique a"><h3><span className="mark">A</span>仅样本有<small>{diff.onlyLeft.length}</small></h3>{diff.onlyLeft.length ? <Tokens items={diff.onlyLeft} quality={tab === "positive"} /> : <p className="empty-tokens">没有独有项</p>}</section><section className="unique b"><h3><span className="mark">B</span>仅目标有<small>{diff.onlyRight.length}</small></h3>{diff.onlyRight.length ? <Tokens items={diff.onlyRight} quality={tab === "positive"} /> : <p className="empty-tokens">没有独有项</p>}</section></div>
            {diff.shared.length > 0 && <><button className="shared-toggle" aria-expanded={showShared} onClick={() => setShowShared(!showShared)}>{showShared ? "收起" : "展开"}共有提示词<span>{diff.shared.length}</span></button>{showShared && <div className="shared"><Tokens items={diff.shared} quality={tab === "positive"} /></div>}</>}
            {tab === "positive" && <p className="quality-note">官方质量词淡化显示；重复的提示词按实际次数比较。</p>}
          </>}</div>
        </aside>
      </div>
    </div>
    {originalRow && <OriginalLightbox key={originalRow.id} row={originalRow} onClose={() => setOriginalRow(null)} />}
  </>;
}
