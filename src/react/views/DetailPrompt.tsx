import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Check, Copy, Filter } from "lucide-react";
import type { RowRecord } from "../../lib/api";
import { errorText } from "../../lib/utils/format";
import { setDetailPromptMode, useDetailPreferences } from "../state/detail-preferences";
import { notify } from "../state/notices";
import { filterByArtists } from "../state/row-actions";
import { ArtistText } from "../ui/ArtistText";
import { Button, Hint } from "../ui/controls";
import { FieldEditor } from "../ui/FieldEditor";
import { LlmBadge } from "../ui/LlmBadge";
import "./detail-prompt.css";

export function DetailPrompt({ row }: { row: RowRecord }) {
  const container = useRef<HTMLDivElement>(null);
  const [expanded, setExpanded] = useState(false);
  const [overflow, setOverflow] = useState(false);
  const mode = useDetailPreferences(state => state.promptMode);
  const [copied, setCopied] = useState(false);
  const copyTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(copyTimer.current), []);
  useLayoutEffect(() => {
    const root = container.current;
    if (!root) return;
    const measure = () => {
      const text = root.querySelector<HTMLElement>(".r-field-body > .r-prompt");
      setOverflow(Boolean(text && (expanded || text.scrollHeight > text.clientHeight + 1)));
    };
    const resize = new ResizeObserver(measure);
    resize.observe(root);
    const mutation = new MutationObserver(measure);
    mutation.observe(root, { childList: true, subtree: true, characterData: true });
    measure();
    return () => { resize.disconnect(); mutation.disconnect(); };
  }, [mode, expanded]);
  const hasArtists = Boolean(row.artists?.trim());
  const copyArtists = async () => {
    try {
      await navigator.clipboard.writeText(row.artists ?? "");
      setCopied(true); clearTimeout(copyTimer.current);
      copyTimer.current = setTimeout(() => setCopied(false), 1200);
    } catch (error) { notify(`复制失败：${errorText(error)}`, "error"); }
  };
  const heading = <div className="rd-prompt-switch" role="group" aria-label="提示词显示（所有图片同步，自动记住）" data-mode={mode}>
    <span className="rd-prompt-thumb" aria-hidden="true" />
    {([['positive', '正向提示词'], ['artists', '画师串']] as const).map(([value, label]) => <Hint key={value} text="切换所有图片的展示内容，自动记住选择">
      <button type="button" aria-pressed={mode === value} onClick={() => setDetailPromptMode(value)}>{label}</button>
    </Hint>)}
  </div>;
  return <div ref={container} className="rd-prompt-container" data-expanded={expanded}><FieldEditor row={row} field="positivePrompt" label="正向提示词" heading={heading} className="rd-prompt-section" alternate={mode === "artists" ? {
    actions: <>
      <Hint text="筛选相同画师串"><Button size="icon" variant="ghost" disabled={!hasArtists} aria-label="筛选相同画师串" onClick={() => filterByArtists(row.artists!)}><Filter size={13} /></Button></Hint>
      <Hint text="复制画师串"><Button size="icon" variant="ghost" disabled={!hasArtists} aria-label="复制画师串" onClick={() => void copyArtists()}>{copied ? <Check size={13} /> : <Copy size={13} />}</Button></Hint>
    </>,
    content: <>
      {row.artistLlm && <div className="rd-prompt-source"><LlmBadge source={row.artistLlm} /></div>}
      <p className={hasArtists ? "r-prompt" : "r-muted"}>{hasArtists ? <ArtistText text={row.artists!} /> : "暂无画师串"}</p>
    </>,
  } : undefined} />{overflow && <Button className="rd-prompt-expand" variant="ghost" size="sm" aria-expanded={expanded} onClick={() => setExpanded(value => !value)}>{expanded ? "收起全文" : "展开全文"}</Button>}</div>;
}
