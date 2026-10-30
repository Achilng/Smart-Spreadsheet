import { useEffect, useRef, useState } from "react";
import { Check, Copy } from "lucide-react";
import { getRowCharacterPrompts, type RowCharacterPrompts, type RowRecord } from "../../lib/api";
import { errorText } from "../../lib/utils/format";
import { useImageCache } from "../state/image-cache";
import { useLibrary } from "../state/library";
import { notify } from "../state/notices";
import { Button, Hint } from "../ui/controls";
import { FieldEditor } from "../ui/FieldEditor";
import { DetailFold, useDetailFoldOpen } from "./DetailOrganization";
import "./detail-prompt.css";

export function DetailCharacters({ row }: { row: RowRecord }) {
  const open = useDetailFoldOpen("characterPrompt");
  const directory = useLibrary(state => state.snapshot?.dataDirectory);
  const version = useImageCache(state => state.version);
  const sourceKey = JSON.stringify([directory, row.id, row.imagePath, row.storedImagePath, version]);
  const [result, setResult] = useState<{ key: string; data?: RowCharacterPrompts; error?: string }>();
  const [attempt, setAttempt] = useState(0);
  const [copied, setCopied] = useState<string>();
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const current = result?.key === sourceKey ? result : undefined;
  useEffect(() => () => clearTimeout(timer.current), []);
  useEffect(() => {
    if (!open) return;
    let disposed = false;
    setResult(undefined);
    void getRowCharacterPrompts(row.id).then(
      data => { if (!disposed) setResult({ key: sourceKey, data }); },
      error => { if (!disposed) setResult({ key: sourceKey, error: errorText(error) }); },
    );
    return () => { disposed = true; };
  }, [open, sourceKey, row.id, attempt]);
  const data = current?.data;
  const matches = data && (row.characterPrompt ?? "").trim() === data.characterPrompt.trim();
  const grouped = matches && data.characters.length > 0;
  const copy = async (key: string, text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(key); clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(undefined), 1200);
    } catch (error) { notify(`复制失败：${errorText(error)}`, "error"); }
  };
  const display = grouped ? <div className="rd-characters">{data.characters.map(character => <div className="rd-character" key={character.index}>
    <h5>角色 {character.index}</h5>
    {([['positivePrompt', '正向提示词'], ['negativePrompt', '负向提示词']] as const).map(([field, label]) => {
      const text = character[field];
      if (field === "negativePrompt" && !text.trim()) return null;
      const key = `${character.index}-${field}`;
      const copyLabel = `复制角色 ${character.index} ${label}`;
      return <div key={field} className="rd-character-field"><div className="r-section-heading"><span>{label}</span><Hint text={copyLabel}><Button size="icon" variant="ghost" aria-label={copyLabel} disabled={!text.trim()} onClick={() => void copy(key, text)}>{copied === key ? <Check size={13} /> : <Copy size={13} />}</Button></Hint></div><p className={text.trim() ? "r-prompt" : "r-muted"}>{text.trim() ? text : "未填写"}</p></div>;
    })}
  </div>)}</div> : undefined;
  return <DetailFold name="characterPrompt" label="角色提示词" summary={grouped ? `${data.characters.length} 个角色` : row.characterPrompt || "未填写"}>
    {!current && open && <p className="r-muted" role="status">正在读取角色分组…</p>}
    {current?.error && <div className="rd-character-status" role="status"><p className="r-muted">无法读取角色分组，已保留当前文本。{current.error}</p><Button size="sm" variant="ghost" onClick={() => setAttempt(value => value + 1)}>重试</Button></div>}
    {data && !grouped && <p className="r-muted">{data.characters.length ? "当前文本与原图不同，无法准确对应角色，按现有文本显示。" : "原图未记录角色分组，按现有文本显示。"}</p>}
    <FieldEditor row={row} field="characterPrompt" label="角色提示词" display={display} />
  </DetailFold>;
}
