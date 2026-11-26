import { memo, useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { ArrowUpRight, Images, Layers } from "lucide-react";
import type { RowRecord } from "../../../lib/api";
import { compareFields, describeChange, orderedRows, startingConditions, type ChangeSummary, type CompareField } from "../../../lib/api/compare-conditions";
import { modelVersionBadge } from "../../../lib/utils/model-version";
import { rowFileName, rowResolution } from "../../../lib/utils/row-display";
import { formatCount, errorText } from "../../../lib/utils/format";
import { compareModelGroups, loadCompareModels, loadCompareSection, openSideBySide, retryCompareSection, setCompareOptions, setExpandedGroup, useCompare, type CompareSectionKey } from "../../state/compare";
import { Button, Modal } from "../../ui/controls";
import { CompareGrid, type GridSection } from "./CompareGrid";
import { CompareConditionsBar } from "./CompareConditionsBar";
import { CompareRowContextMenu } from "./CompareRowContextMenu";
import { PaneImage } from "./CompareImages";

/** Which field each fixed relation leaves free to differ, shown under every card. */
const RELATION_FIELD: Record<Exclude<CompareSectionKey, "custom">, CompareField> = { artists: "positive", vibeDiffStyle: "positive", styleDiffVibe: "vibe" };
const RELATION_COPY: Record<Exclude<CompareSectionKey, "custom">, { title: string; description: string }> = {
  artists: { title: "相同画师串", description: "完整画师串一致，卡片下方是提示词和 A 的差别。" },
  vibeDiffStyle: { title: "相同 VIBE · 不同提示词", description: "VIBE 引用不变，卡片下方是提示词和 A 的差别。" },
  styleDiffVibe: { title: "相同提示词 · 不同 VIBE", description: "正向提示词一致（忽略官方质量词），观察 VIBE 引用带来的变化。" },
};
const MAX_CHIPS = 40;

/** A card to bring into view and focus; `memberId` is the group member to focus when the group dialog reopens. */
export type Reveal = { rowId: number; memberId?: number } | null;
export const rowDetails = (row: RowRecord) => [[rowFileName(row), rowResolution(row), `#${row.id}`].filter(Boolean).join(" · "), row.imagePath].filter(Boolean).join("\n");

export function Badges({ row }: { row: RowRecord }) {
  const badge = modelVersionBadge(row.generationModel);
  return <>{badge && <span className={`version-badge ${badge.className}`}>{badge.label}</span>}{Boolean(row.vibeReferenceCount) && <span className="vibe-badge">VIBE ×{row.vibeReferenceCount}</span>}</>;
}

/** B's difference from A: green tokens B added, struck-through red ones it dropped, clamped with an ellipsis. */
function ChangeLine({ change }: { change: ChangeSummary }) {
  return <p className="card-diff" title={change.full}>
    {change.tokens.length ? change.tokens.slice(0, MAX_CHIPS).map((token, index) => <span key={index} className={`diff-chip is-${token.kind}${token.quality ? " is-quality" : ""}`}>{token.text}</span>)
      : <span className="diff-text">{change.text}</span>}
  </p>;
}

const CompareCard = memo(function CompareCard({ row, sample, field, group }: {
  row: RowRecord; sample: RowRecord; field: CompareField | null; group?: number[];
}) {
  const change = useMemo(() => field ? describeChange(sample, row, field) : null, [sample, row, field]);
  return <CompareRowContextMenu row={row}><article className="rc-card">
    <div className="thumb-wrap"><div className="thumb">
      <PaneImage row={row} tier="gallery" lazy />
      <div className="card-badges"><Badges row={row} /></div>
      <button type="button" className="card-open" data-compare-id={row.id} aria-label={`和 A 并排对比：${rowFileName(row) || `#${row.id}`}`} title={rowDetails(row)} onClick={() => openSideBySide(row)}>
        <span className="open-hint"><ArrowUpRight size={14} />对比这张</span>
      </button>
      {group && <button type="button" className="stack-count" data-stack-id={row.id} aria-haspopup="dialog" title={`差别相同的 ${group.length} 张，点开查看`} onClick={() => setExpandedGroup(group)}>
        <Layers size={12} />共 {formatCount(group.length)} 张
      </button>}
    </div></div>
    {change && <ChangeLine change={change} />}
  </article></CompareRowContextMenu>;
});

/** All members of one deduplicated group, in a dialog over the results. */
function GroupDialog({ ids, field, focusId }: { ids: number[]; field: CompareField; focusId?: number }) {
  const [rows, setRows] = useState<RowRecord[] | null>(null);
  const [error, setError] = useState("");
  const [attempt, setAttempt] = useState(0);
  const grid = useRef<HTMLDivElement>(null);
  useEffect(() => {
    let active = true;
    setError("");
    void orderedRows(ids).then(list => { if (active) setRows(list); }).catch(cause => { if (active) setError(errorText(cause)); });
    return () => { active = false; };
  }, [ids, attempt]);
  useEffect(() => {
    // Back from the side-by-side view, focus the B that was last open.
    if (rows && focusId !== undefined) grid.current?.querySelector<HTMLElement>(`[data-compare-id="${focusId}"]`)?.focus();
  }, [rows, focusId]);
  const close = () => {
    setExpandedGroup(null);
    requestAnimationFrame(() => document.querySelector<HTMLElement>(`[data-stack-id="${ids[0]}"]`)?.focus());
  };
  return <Modal open onClose={close} width={980} className="rc-group-dialog" title={`差别相同的 ${formatCount(ids.length)} 张`}
    description={`和 A 的${compareFields[field]}差别一样，其它允许不同的条件可能各不相同。点一张和 A 并排对比。`}>
    {error ? <p className="group-error" role="alert">{error}<Button size="sm" onClick={() => setAttempt(attempt + 1)}>重试</Button></p>
      : !rows ? <p className="group-loading" role="status"><span className="rc-image-loading" />正在读取这一组…</p>
        : <div className="group-grid" ref={grid}>{rows.map(row => <CompareRowContextMenu row={row} key={row.id}>
          <button type="button" className="group-thumb" data-compare-id={row.id} title={rowDetails(row)} aria-label={`和 A 并排对比：${rowFileName(row) || `#${row.id}`}`} onClick={() => openSideBySide(row, false, rows)}>
            <PaneImage row={row} tier="gallery" lazy />
            <span className="card-badges"><Badges row={row} /></span>
          </button>
        </CompareRowContextMenu>)}</div>}
  </Modal>;
}

function GridFooter({ loaded, total, unit, error, onRetry }: { loaded: number; total: number; unit: string; error: string | null; onRetry: () => void }) {
  if (error) return <div className="grid-footer is-error" role="alert"><span>读取更多结果失败：{error}</span><Button size="sm" onClick={onRetry}>重试</Button></div>;
  if (loaded < total) return <div className="grid-footer" role="status"><span className="rc-image-loading" />正在加载更多…</div>;
  return <div className="grid-footer is-done">已显示全部 {formatCount(total)} {unit}</div>;
}

function Skeleton() {
  return <div className="skeleton-grid" role="status" aria-label="正在加载关联图片">{Array.from({ length: 8 }, (_, i) => <div className="skeleton" key={i} />)}</div>;
}

export function CompareSection({ section, scrollRef, reveal }: { section: CompareSectionKey; scrollRef: RefObject<HTMLElement | null>; reveal: Reveal }) {
  const state = useCompare(s => s[section]);
  const sample = useCompare(s => s.sample)!;
  const options = useCompare(s => s.options);
  const groups = useCompare(s => s.groups);
  const expandedGroup = useCompare(s => s.expandedGroup);
  const hasTarget = useCompare(s => s.target !== null);
  const custom = section === "custom";
  const grouped = custom && options.dedupe;
  const field = custom ? options.observe : RELATION_FIELD[section];
  const groupOf = useMemo(() => grouped && groups ? new Map(groups.map(group => [group[0], group])) : null, [grouped, groups]);
  const imageCount = useMemo(() => grouped && groups ? groups.reduce((sum, group) => sum + group.length, 0) : null, [grouped, groups]);
  const unavailable = custom ? false : section === "artists" ? !sample.row.artists?.trim() : section === "vibeDiffStyle" ? !sample.hasVibeSignature : !sample.hasStyleSignature;
  const missing = section === "artists" ? "样本还没有画师串，试试其他对比关系。" : section === "vibeDiffStyle" ? sample.vibeSignatureUnreadable ? "样本原图不可读，无法读取 VIBE 引用。" : "样本没有引用 VIBE，试试其他对比关系。" : "样本没有可比较的提示词，试试其他对比关系。";
  const title = custom ? `比较${compareFields[options.observe]}` : RELATION_COPY[section].title;
  // New conditions keep the old results on screen, dimmed, until the new ones arrive.
  const refreshing = custom && state.loading && !groups && state.loaded;
  const gridSections = useMemo<GridSection[]>(() => [{ key: section, rows: state.items }], [section, state.items]);
  const onNearEnd = useCallback(() => void loadCompareSection(section), [section]);
  const openGroup = expandedGroup && groupOf?.get(expandedGroup[0]) === expandedGroup ? expandedGroup : null;
  const unit = grouped ? "组" : "张";
  return <section className={`rc-section${refreshing ? " is-refreshing" : ""}`} aria-busy={state.loading}>
    <header className="section-head">
      <div className="section-title">
        <div className="section-copy">
          <h1>{title}{state.loaded && <span className="section-count">{grouped ? `${formatCount(state.total)} 组 · 共 ${formatCount(imageCount ?? 0)} 张` : `${formatCount(state.total)} 张`}</span>}</h1>
          {!custom && <p>{RELATION_COPY[section].description}</p>}
        </div>
        {!custom && <Button size="sm" onClick={() => setCompareOptions(startingConditions(section))} title="以这个关系为起点，自己调整比较条件">自定义条件</Button>}
      </div>
      {custom && <CompareConditionsBar options={options} />}
    </header>
    {state.items.length ? <CompareGrid scrollRef={scrollRef} sections={gridSections} onNearEnd={onNearEnd} reveal={reveal}
      footer={<GridFooter loaded={state.items.length} total={state.total} unit={unit} error={state.error} onRetry={() => retryCompareSection(section)} />}
      renderCard={row => {
        const group = groupOf?.get(row.id);
        return <CompareCard row={row} sample={sample.row} field={field} group={group && group.length > 1 ? group : undefined} />;
      }} />
      : state.error ? <div className="error" role="alert"><span>{state.error}</span><Button onClick={() => retryCompareSection(section)}>重新加载</Button></div>
        : state.loading || !state.loaded ? <Skeleton />
          : <div className="empty"><Images size={28} /><h2>{unavailable ? "缺少对比信息" : "暂时没有匹配图片"}</h2><p>{custom ? "换一个要比较的项，或者放宽要和 A 一致的条件。" : unavailable ? missing : "资料库中还没有符合这个关系的图片，可以切换其他关系继续探索。"}</p></div>}
    {openGroup && !hasTarget && <GroupDialog key={openGroup[0]} ids={openGroup} field={field} focusId={reveal?.memberId} />}
  </section>;
}

export function ModelSection({ scrollRef, reveal }: { scrollRef: RefObject<HTMLElement | null>; reveal: Reveal }) {
  const { models, modelsLoading, modelsError, sample } = useCompare();
  const groups = compareModelGroups(models, sample?.row);
  const total = groups.reduce((sum, group) => sum + group.rows.length, 0);
  const gridSections = useMemo<GridSection[]>(() => groups.map(group => ({
    key: group.tier, rows: group.rows,
    heading: <h2 className="grid-heading">{group.title}<span>{formatCount(group.rows.length)} 张</span></h2>,
  })), [groups]);
  return <section className="rc-section" aria-busy={modelsLoading}>
    <header className="section-head">
      <div className="section-title">
        <div className="section-copy">
          <h1>相同提示词 · 不同模型{!modelsLoading && <span className="section-count">{formatCount(total)} 张</span>}</h1>
          <p>正向提示词一致，按作画模型分段，往下滚依次查看。</p>
        </div>
        <Button size="sm" onClick={() => setCompareOptions(startingConditions("models"))} title="以这个关系为起点，自己调整比较条件">自定义条件</Button>
      </div>
    </header>
    {models.truncated && <div className="truncate-note">仅比较最新 500 张同画风图片。其他模型的结果也可能在更早的图片中。</div>}
    {modelsError ? <div className="error" role="alert">{modelsError}<Button onClick={() => void loadCompareModels()}>重新加载</Button></div>
      : modelsLoading ? <Skeleton />
        : total ? <CompareGrid scrollRef={scrollRef} sections={gridSections} reveal={reveal} renderCard={row => <CompareCard row={row} sample={sample!.row} field={null} />} />
          : <div className="empty"><Images size={28} /><h2>{!sample?.hasStyleSignature ? "缺少对比信息" : "暂时没有其他模型的图片"}</h2><p>{!sample?.hasStyleSignature ? "样本没有可比较的提示词。" : models.truncated ? "最新 500 张同画风图片中，没有其他模型的结果。" : models.totalCount ? "找到的相同画风图片都与样本使用同一模型。" : "资料库中还没有符合条件的图片。"}</p></div>}
  </section>;
}
