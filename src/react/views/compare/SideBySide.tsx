import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, Crosshair, Maximize2 } from "lucide-react";
import type { RowRecord } from "../../../lib/api";
import { diffPromptField, type PromptToken } from "../../../lib/utils/prompt-diff";
import { modelVersionBadge } from "../../../lib/utils/model-version";
import { rowFileName, rowResolution } from "../../../lib/utils/row-display";
import { openRowInMainWindow } from "../../../lib/windows/toolbox";
import { notify } from "../../state/notices";
import { PaneImage, OriginalLightbox } from "./CompareImages";

function Tokens({ items, quality }: { items: PromptToken[]; quality: boolean }) {
  return <div className="tokens">{items.map((token, index) => <span key={index} className={`token ${quality && token.isQuality ? "quality" : ""}`}>{token.display}</span>)}</div>;
}
const value = (input: string | number | null | undefined) => input == null || input === "" ? "—" : String(input);

export function SideBySide({ sample, target, onBack }: { sample: RowRecord; target: RowRecord; onBack: () => void }) {
  const [tab, setTab] = useState<"positive" | "character" | "params">("positive");
  const [showShared, setShowShared] = useState(false);
  const [onlyChangedParams, setOnlyChangedParams] = useState(false);
  const [originalRow, setOriginalRow] = useState<RowRecord | null>(null);
  const positive = useMemo(() => diffPromptField(sample.positivePrompt, target.positivePrompt), [sample.positivePrompt, target.positivePrompt]);
  const character = useMemo(() => diffPromptField(sample.characterPrompt, target.characterPrompt), [sample.characterPrompt, target.characterPrompt]);
  const diff = tab === "character" ? character : positive;
  const changes = diff.onlyLeft.length + diff.onlyRight.length;
  const params = [
    ["模型", value(sample.generationModel), value(target.generationModel)],
    ["采样器", value(sample.generationSampler), value(target.generationSampler)],
    ["步数", value(sample.generationSteps), value(target.generationSteps)],
    ["种子", value(sample.generationSeed), value(target.generationSeed)],
    ["Guidance", value(sample.generationScale), value(target.generationScale)],
    ["CFG Rescale", value(sample.generationCfgRescale), value(target.generationCfgRescale)],
    ["噪声调度", value(sample.generationNoiseSchedule), value(target.generationNoiseSchedule)],
    ["尺寸", value(rowResolution(sample)), value(rowResolution(target))],
  ];
  const changedParams = params.filter(row => row[1] !== row[2]).length;
  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !originalRow && !event.defaultPrevented) { event.preventDefault(); onBack(); }
    };
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, [originalRow, onBack]);
  async function locate(rowId: number) {
    try { await openRowInMainWindow(rowId); }
    catch (error) { notify(`无法在主画廊定位：${String(error)}`, "error"); }
  }
  return <><div className="rc-side"><header className="head"><button className="back" onClick={onBack}><ArrowLeft size={16} />返回结果</button><div><strong>双图对照</strong><span>观察画面，核对创作条件</span></div><small>ESC 返回</small></header><div className="comparison-layout"><div className="image-stage"><div className="panes">{[{ label: "样本", mark: "A", row: sample }, { label: "目标", mark: "B", row: target }].map(pane => {
    const hasImage = Boolean(pane.row.imagePath?.trim() || pane.row.storedImagePath?.trim());
    const badge = modelVersionBadge(pane.row.generationModel);
    return <figure className={`pane ${pane.mark === "B" ? "target" : ""}`} key={pane.mark}><figcaption><span className="mark">{pane.mark}</span><strong>{pane.label}</strong><span className="resolution">{rowResolution(pane.row) || "尺寸未知"}</span></figcaption><button className="pane-media" disabled={!hasImage} onClick={() => setOriginalRow(pane.row)} aria-label={`查看${pane.label}原图`}><PaneImage row={pane.row} />{badge && <span className={`version-badge ${badge.className}`}>{badge.label}</span>}{hasImage && <span className="original-hint"><Maximize2 size={13} />查看原图</span>}</button><div className="pane-footer"><p title={pane.row.imagePath || undefined}>{pane.row.note?.trim() || rowFileName(pane.row) || `#${pane.row.id}`}</p><button onClick={() => void locate(pane.row.id)} title="在主画廊定位" aria-label={`在主画廊定位${pane.label}`}><Crosshair size={12} />定位</button></div></figure>;
  })}</div><p className="stage-note">点击图片查看完整原图 · 两侧均保持原始比例</p></div><aside className="inspector" aria-label="图片差异"><header className="inspector-head"><span>差异检查</span><small>A 样本 / B 目标</small></header><nav className="diff-tabs" aria-label="差异类别">{([['positive', '正向提示词'], ['character', '角色提示词'], ['params', '生成参数']] as const).map(([key, label]) => <button key={key} className={tab === key ? "active" : ""} aria-pressed={tab === key} onClick={() => setTab(key)}>{label}{key === "params" && <span>{changedParams}</span>}</button>)}</nav><div className="inspector-scroll">{tab === "params" ? <><div className="diff-summary"><strong>{changedParams} 项参数不同</strong><button aria-pressed={onlyChangedParams} onClick={() => setOnlyChangedParams(!onlyChangedParams)}>{onlyChangedParams ? "显示全部" : "只看不同"}</button></div>{onlyChangedParams && !changedParams && <div className="empty">两张图片的生成参数完全一致。</div>}<div className="parameter-list">{params.filter(row => !onlyChangedParams || row[1] !== row[2]).map(row => <section key={row[0]} className={row[1] !== row[2] ? "changed" : ""}><h3>{row[0]}{row[1] !== row[2] && <span>不同</span>}</h3><div><p><small>A</small>{row[1]}</p><p><small>B</small>{row[2]}</p></div></section>)}</div></> : <><div className="diff-summary"><strong>{changes ? `${changes} 项提示词不同` : "提示词一致"}</strong><span>{diff.shared.length} 项共有</span></div>{changes === 0 && <div className="same-note">{diff.shared.length ? "两侧提示词项一致，可以继续比较生成参数。" : `两侧${tab === "character" ? "角色" : "正向"}提示词都为空。`}</div>}<div className="unique-columns"><section className="unique a"><h3><span>A</span>仅样本有<small>{diff.onlyLeft.length}</small></h3>{diff.onlyLeft.length ? <Tokens items={diff.onlyLeft} quality={tab === "positive"} /> : <p className="empty">没有独有项</p>}</section><section className="unique b"><h3><span>B</span>仅目标有<small>{diff.onlyRight.length}</small></h3>{diff.onlyRight.length ? <Tokens items={diff.onlyRight} quality={tab === "positive"} /> : <p className="empty">没有独有项</p>}</section></div>{diff.shared.length > 0 && <><button className="shared-toggle" aria-expanded={showShared} onClick={() => setShowShared(!showShared)}>{showShared ? "收起" : "展开"}双方共有内容<span>{diff.shared.length}</span></button>{showShared && <div className="shared"><Tokens items={diff.shared} quality={tab === "positive"} /></div>}</>}{tab === "positive" && <p className="quality-note">官方质量词淡化显示，重复出现的提示词按实际次数比较。</p>}</>}</div></aside></div></div>{originalRow && <OriginalLightbox key={originalRow.id} row={originalRow} onClose={() => setOriginalRow(null)} />}</>;
}
