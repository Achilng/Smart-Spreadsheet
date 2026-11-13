import { Fragment, type ReactNode } from "react";

export const FIELD_LABELS: Record<string, string> = {
  positive: "正向", character: "角色", negative: "负面", artists: "画师串", note: "备注",
  group: "分组", favorite: "收藏", tagsAdded: "新增 TAG", tagsRemoved: "移除 TAG",
};

export const TOOL_LABELS: Record<string, string> = {
  get_library_overview: "查看图库概况",
  get_main_window_state: "读取主窗口状态",
  find_rows: "查看图片内容",
  analyze_text: "统计文字出现情况",
  list_artist_strings: "查找画师串",
  preview_changes: "预演修改",
  apply_changes: "执行修改",
  undo_operation: "撤销修改",
  list_operations: "查看操作记录",
  set_main_selection: "在主窗口选中图片",
  set_main_filters: "设置主窗口筛选",
  create_automation_rule: "创建自动规则",
};

const SCOPE_LABELS: Record<string, string> = { selection: "主窗口当前选中", filteredView: "主窗口当前筛选结果", all: "全库", rowIds: "指定图片" };

const str = (value: unknown) => (typeof value === "string" ? value : value == null ? "" : JSON.stringify(value));
const quote = (value: unknown) => `「${str(value)}」`;
const field = (value: unknown) => FIELD_LABELS[str(value)] ?? str(value);
const caseText = (value: unknown) => (value ? "区分大小写" : "不区分大小写");

export function describeTarget(target: Record<string, unknown> | undefined): string[] {
  if (!target) return [];
  const lines = [`范围：${SCOPE_LABELS[str(target.scope)] ?? str(target.scope)}`];
  const ids = target.rowIds as number[] | undefined;
  if (target.scope === "rowIds" && ids?.length) lines.push(`图片 ID：${ids.slice(0, 12).join("、")}${ids.length > 12 ? ` 等 ${ids.length} 张` : ""}`);
  const artists = target.artistStrings as string[] | undefined;
  if (artists?.length) lines.push(`只限画师串：${artists.map(quote).join("、")}`);
  if (target.expandToSameArtistString) lines.push("扩展到同画师串的所有图");
  if (typeof target.favorite === "boolean") lines.push(target.favorite ? "只限已收藏" : "只限未收藏");
  const conditions = target.conditions as Record<string, unknown>[] | undefined;
  if (conditions?.length) lines.push(`附加条件（${target.conditionMatch === "any" ? "满足任一" : "全部满足"}）：${conditions.map(describeCondition).join("；")}`);
  return lines;
}

function describeCondition(condition: Record<string, unknown>): string {
  const op = str(condition.operator);
  switch (condition.type) {
    case "prompt": return `提示词(${str(condition.scope)}) ${op} ${quote(condition.value)}`;
    case "tag": return `TAG ${op} ${((condition.tags as string[]) ?? []).map(quote).join("、")}`;
    case "group": return `分组 ${op} ${str(condition.groupId)}`;
    case "note": return `备注 ${op} ${quote(condition.value)}`;
    case "artist": return `画师 ${op} ${((condition.artists as string[]) ?? []).map(quote).join("、")}`;
    default: return JSON.stringify(condition);
  }
}

export function describeAction(action: Record<string, unknown>): string {
  switch (action.type) {
    case "addTags": return `添加 TAG：${((action.tags as string[]) ?? []).map(quote).join("、")}`;
    case "removeTags": return `移除 TAG：${((action.tags as string[]) ?? []).map(quote).join("、")}`;
    case "setGroup": return `移到分组 ${quote(action.groupName)}${action.onlyIfUngrouped ? "（只处理还没分组的图）" : ""}`;
    case "clearGroup": return "移出分组";
    case "appendPrompt": return `在${field(action.field)}末尾追加 ${quote(action.value)}`;
    case "deletePromptTags": return `从${field(action.field)}删除 TAG ${quote(action.value)}`;
    case "replacePrompt": return `把${field(action.field)}里的 ${quote(action.find)} 替换为 ${action.replace ? quote(action.replace) : "空（删除）"}，${caseText(action.caseSensitive)}`;
    case "regexReplacePrompt": return `在${field(action.field)}里用正则 ${quote(action.pattern)} 替换为 ${action.replace ? quote(action.replace) : "空（删除）"}，${caseText(action.caseSensitive)}`;
    case "setNote": return `备注改为 ${quote(action.value)}`;
    case "appendNote": return `备注追加 ${quote(action.value)}`;
    case "clearNote": return "清空备注";
    case "setNoteSequence": return `备注按顺序编号，前缀 ${quote(action.prefix)}`;
    case "setFavorite": return action.favorite ? "设为收藏" : "取消收藏";
    default: return JSON.stringify(action);
  }
}

export function formatTime(value: string): string {
  const date = new Date(value.endsWith("Z") || value.includes("+") ? value : `${value}Z`);
  if (Number.isNaN(date.getTime())) return value;
  const pad = (n: number) => String(n).padStart(2, "0");
  const today = new Date();
  const time = `${pad(date.getHours())}:${pad(date.getMinutes())}`;
  return date.toDateString() === today.toDateString() ? time : `${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${time}`;
}

/** Show the edited span of a before/after pair: unchanged context plus removed / inserted text. */
export function DiffPair({ before, after, afterEmpty }: { before: string; after: string; afterEmpty?: boolean }) {
  const a = Array.from(before), b = Array.from(after);
  let prefix = 0;
  while (prefix < a.length && prefix < b.length && a[prefix] === b[prefix]) prefix++;
  let suffix = 0;
  while (suffix < a.length - prefix && suffix < b.length - prefix && a[a.length - 1 - suffix] === b[b.length - 1 - suffix]) suffix++;
  const head = a.slice(0, prefix).join(""), tail = a.slice(a.length - suffix).join("");
  const removed = a.slice(prefix, a.length - suffix).join(""), added = b.slice(prefix, b.length - suffix).join("");
  return <div className="ag-diff">
    <div className="ag-diff-line"><span className="ag-diff-tag">前</span><span className="ag-diff-text">{head}{removed && <del>{removed}</del>}{tail}{!before && <em>（空）</em>}</span></div>
    <div className="ag-diff-line"><span className="ag-diff-tag is-after">后</span><span className="ag-diff-text">{head}{added && <ins>{added}</ins>}{tail}{(afterEmpty || !after) && <em>（空）</em>}</span></div>
  </div>;
}

function inline(text: string, key: string): ReactNode[] {
  const parts: ReactNode[] = [];
  const pattern = /(\*\*[^*]+\*\*|`[^`]+`)/g;
  let last = 0, index = 0;
  for (const match of text.matchAll(pattern)) {
    if (match.index > last) parts.push(text.slice(last, match.index));
    const token = match[0];
    parts.push(token.startsWith("**") ? <strong key={`${key}-${index++}`}>{token.slice(2, -2)}</strong> : <code key={`${key}-${index++}`}>{token.slice(1, -1)}</code>);
    last = match.index + token.length;
  }
  if (last < text.length) parts.push(text.slice(last));
  return parts;
}

/** A small, safe markdown subset: paragraphs, headings, bullet / numbered lists, bold and inline code. */
export function RichText({ text }: { text: string }) {
  const blocks: ReactNode[] = [];
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  let list: { ordered: boolean; items: string[] } | null = null;
  let paragraph: string[] = [];
  const flushParagraph = () => {
    if (paragraph.length) blocks.push(<p key={`p${blocks.length}`}>{paragraph.map((line, i) => <Fragment key={i}>{i > 0 && <br />}{inline(line, `p${blocks.length}-${i}`)}</Fragment>)}</p>);
    paragraph = [];
  };
  const flushList = () => {
    if (list) {
      const items = list.items.map((item, i) => <li key={i}>{inline(item, `l${blocks.length}-${i}`)}</li>);
      blocks.push(list.ordered ? <ol key={`l${blocks.length}`}>{items}</ol> : <ul key={`l${blocks.length}`}>{items}</ul>);
    }
    list = null;
  };
  for (const raw of lines) {
    const line = raw.trimEnd();
    const bullet = /^\s*[-*•]\s+(.*)$/.exec(line), numbered = /^\s*\d+[.)、]\s+(.*)$/.exec(line), heading = /^#{1,4}\s+(.*)$/.exec(line);
    if (bullet || numbered) {
      flushParagraph();
      const ordered = !!numbered;
      if (!list || list.ordered !== ordered) { flushList(); list = { ordered, items: [] }; }
      list.items.push((bullet ?? numbered)![1]);
    } else if (heading) {
      flushParagraph(); flushList();
      blocks.push(<h4 key={`h${blocks.length}`}>{inline(heading[1], `h${blocks.length}`)}</h4>);
    } else if (!line.trim()) {
      flushParagraph(); flushList();
    } else {
      flushList();
      paragraph.push(line);
    }
  }
  flushParagraph(); flushList();
  return <div className="ag-rich">{blocks}</div>;
}
