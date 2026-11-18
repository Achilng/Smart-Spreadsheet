import { Fragment, memo, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { cjk } from "@streamdown/cjk";
import { ArrowDown, ArrowUp, Check, ChevronDown, Copy, FileText, History, Images, MoreHorizontal, PanelLeft, Paperclip, Sparkles, SquarePen } from "lucide-react";
import { attachmentAccept, attachmentNames, DraftAttachment, encodeAttachment, validateFiles } from "./AgentAttachments";
import { Streamdown, defaultRemarkPlugins, type StreamdownProps } from "streamdown";
import { GalleryLink, isGalleryLink, remarkGalleryImages } from "./AgentGalleryLinks";
import { useStickToBottom } from "use-stick-to-bottom";
import {
  agentCancel, agentMessages, agentOperations, agentSend, agentState, applyAgentPreview, deleteAgentConversation, renameAgentConversation, selectedAgentModel,
  type AgentEvent, type AgentMessage, type AgentOperation, type AgentPreviewReport, type AgentState,
} from "../../../lib/api/agent";
import { errorText } from "../../../lib/utils/format";
import { Button, Menu, Modal, Tooltip } from "../../ui/controls";
import { Notices } from "../../ui/Notices";
import { WindowControls } from "../../ui/WindowControls";
import { notify } from "../../state/notices";
import { thumbnails } from "../../../lib/images/thumbnails";
import { clearProgressiveImages } from "../../../lib/images/progressive-images";
import { OperationCard, PreviewCard } from "./AgentCards";
import { AgentPreferencesDialog } from "./AgentPreferencesDialog";
import { ImageViewerProvider, ThumbStrip, useImageViewer } from "./AgentMedia";
import { ThoughtLine, type ThoughtStep } from "./ThoughtLine";
import { TOOL_LABELS, formatTime } from "./agent-format";
import "./agent.css";

// The window carries its own palette; the class also reaches dialogs and menus portalled to <body>.
document.documentElement.classList.add("agent-doc");

const EFFORT_SHORT: Record<string, string> = { minimal: "最快", low: "低强度", medium: "中强度", high: "高强度", default: "默认强度" };
const SUGGESTIONS = [
  { label: "删掉「Text:」后面的内容", text: "把选中图片正向提示词里「Text:」后面的内容删掉" },
  { label: "给一个画师串的图加 TAG", text: "给某个画师串的所有图加一个 TAG" },
  { label: "同时选中几个画师串的图", text: "在主窗口同时选中几个画师串的图" },
  { label: "还有多少图没分组？", text: "图库里有多少张图还没分组？" },
];
const NO_LIVE: Live = { text: "", reasoning: "", tool: null };
const SIDEBAR_KEY = "agent-sidebar-collapsed";

type Live = { text: string; reasoning: string; tool: string | null };
type StepItem = { type: "step"; id: number; at: number; doneAt: number | null; name: string; args: string; output: string | null; ok: boolean; display: Record<string, unknown> | null };
type ReasoningItem = { type: "reasoning"; id: number; at: number; text: string };
type Item =
  | { type: "user"; id: number; at: number; text: string; files: string[] }
  | { type: "assistant"; id: number; at: number; text: string }
  | { type: "error"; id: number; at: number; text: string; stopped: boolean }
  | { type: "action"; id: number; at: number; action: string; title: string; operationId: number | null }
  | ReasoningItem
  | StepItem;
/** A run of reasoning and tool steps, shown as one thought line. */
type ThoughtBlock = { type: "thought"; key: string; entries: (ReasoningItem | StepItem)[]; startedAt: number | null; endedAt: number | null };
type Block = Exclude<Item, ReasoningItem | StepItem> | ThoughtBlock;

function timestamp(value: string): number {
  const time = Date.parse(value.endsWith("Z") || value.includes("+") ? value : `${value}Z`);
  return Number.isNaN(time) ? 0 : time;
}

/** Readable text of the reasoning a model returned with a message, when the provider exposes it. */
function reasoningText(value: unknown): string {
  if (!Array.isArray(value)) return "";
  return value
    .flatMap(item => [...(Array.isArray(item?.summary) ? item.summary : []), ...(Array.isArray(item?.content) ? item.content : [])])
    .map(part => (typeof part?.text === "string" ? part.text.trim() : ""))
    .filter(Boolean)
    .join("\n\n");
}

function toItems(messages: AgentMessage[]): Item[] {
  const items: Item[] = [];
  const steps = new Map<string, StepItem>();
  for (const message of messages) {
    const content = message.content;
    const at = timestamp(message.createdAt);
    const reasoning = reasoningText(content.reasoning);
    if (reasoning) items.push({ type: "reasoning", id: message.id, at, text: reasoning });
    if (message.kind === "user") items.push({ type: "user", id: message.id, at, text: String(content.text ?? ""), files: attachmentNames(content.attachments) });
    else if (message.kind === "assistant") items.push({ type: "assistant", id: message.id, at, text: String(content.text ?? "") });
    else if (message.kind === "user_action") items.push({ type: "action", id: message.id, at, action: String(content.action ?? ""), title: String(content.title ?? ""), operationId: typeof content.operationId === "number" ? content.operationId : null });
    else if (message.kind === "error") items.push({ type: "error", id: message.id, at, text: String(content.message ?? ""), stopped: content.stopped === true });
    else if (message.kind === "tool_call") {
      const step: StepItem = { type: "step", id: message.id, at, doneAt: null, name: String(content.name ?? ""), args: String(content.arguments ?? ""), output: null, ok: true, display: null };
      steps.set(String(content.callId), step); items.push(step);
    } else if (message.kind === "tool_result") {
      const step = steps.get(String(content.callId));
      if (step) { step.output = String(content.output ?? ""); step.ok = content.ok !== false; step.display = (content.display as Record<string, unknown>) ?? null; step.doneAt = at; }
    }
  }
  return items;
}

/** Groups items for display; `thinking` adds (or keeps open) a trailing thought block for the work in progress. */
function toBlocks(items: Item[], thinking: boolean): Block[] {
  const blocks: Block[] = [];
  let anchor = "start";
  let clock: number | null = null;
  const thought = () => {
    const last = blocks[blocks.length - 1];
    if (last?.type === "thought") return last;
    const block: ThoughtBlock = { type: "thought", key: `thought-${anchor}`, entries: [], startedAt: clock, endedAt: null };
    blocks.push(block);
    return block;
  };
  for (const item of items) {
    const end = (item.type === "step" ? item.doneAt : null) ?? item.at;
    if (item.type === "reasoning" || item.type === "step") {
      const block = thought();
      block.entries.push(item);
      block.endedAt = end || null;
    } else {
      blocks.push(item);
      anchor = `${item.type}-${item.id}`;
    }
    clock = end || clock;
  }
  if (thinking) thought();
  return blocks;
}

function pretty(text: string): string {
  try { return JSON.stringify(JSON.parse(text), null, 2); } catch { return text; }
}

const MARKDOWN_PLUGINS: StreamdownProps["plugins"] = { cjk };
const GALLERY_PLUGINS = [...Object.values(defaultRemarkPlugins), remarkGalleryImages];
// Local gallery references open the in-window viewer; ordinary links still don't navigate.
const MARKDOWN_COMPONENTS: StreamdownProps["components"] = {
  a: ({ children, href, title }) => typeof href === "string" && isGalleryLink(href)
    ? <GalleryLink href={href} title={title} />
    : <span className="md-link" title={href}>{children}</span>,
};

const Markdown = memo(function Markdown({ text, streaming = false, className = "" }: { text: string; streaming?: boolean; className?: string }) {
  return <Streamdown className={`md${streaming ? " streaming" : ""}${className && ` ${className}`}`} plugins={MARKDOWN_PLUGINS} remarkPlugins={GALLERY_PLUGINS} components={MARKDOWN_COMPONENTS} controls={false} isAnimating={streaming}>{text}</Streamdown>;
});

function numbers(value: unknown): number[] {
  return Array.isArray(value) ? value.filter((id): id is number => typeof id === "number") : [];
}

/** Images whose text a step read (find_rows), inside the folded steps. */
function StepImages({ item }: { item: StepItem }) {
  const view = useImageViewer();
  const ids = item.display?.kind === "rows" ? numbers(item.display.rowIds) : [];
  if (!ids.length) return null;
  return <ThumbStrip ids={ids} max={12} size={48} onOpen={index => view({ title: `助手读取了这 ${ids.length} 张图的信息`, ids, index })} />;
}

/** Images the model actually looked at; stays in the thread even when the steps fold away. */
function ViewedImages({ item }: { item: StepItem }) {
  const view = useImageViewer();
  const [open, setOpen] = useState(true);
  const ids = numbers(item.display?.rowIds);
  const missing = numbers(item.display?.missing).length;
  if (!ids.length) return null;
  return <div className="msg msg-viewed">
    <button type="button" className="msg-viewed__head" aria-expanded={open} onClick={() => setOpen(!open)}>
      <Images className="icon sm" aria-hidden="true" />
      <span>已查看 {ids.length} 张图片{missing ? `（${missing} 张读取不到）` : ""}</span>
      <ChevronDown className="icon sm msg-viewed__chevron" aria-hidden="true" />
    </button>
    {open && <ThumbStrip ids={ids} max={16} size={72} onOpen={index => view({ title: `助手看了这 ${ids.length} 张图`, ids, index })} />}
  </div>;
}

function stepSummary(item: StepItem, interrupted: boolean, failure: string): string {
  const display = item.display;
  if (display?.kind === "selection") return `已选中 ${display.count} 张`;
  if (display?.kind === "images") {
    const count = numbers(display.rowIds).length;
    const missing = numbers(display.missing).length;
    return `${count} 张${count > 2 ? "（拼成一张总览图）" : ""}${missing ? `，${missing} 张读取不到` : ""}`;
  }
  if (display?.kind === "rows") {
    const count = numbers(display.rowIds).length;
    const total = typeof display.total === "number" ? display.total : count;
    return total > count ? `${count} 张（范围内共 ${total} 张）` : `${count} 张`;
  }
  if (interrupted) return "被中断，没有执行";
  return !item.ok && !failure ? "没有成功" : "";
}

/** One tool call inside a thought line; the raw arguments and result open on click. */
function StepRow({ item, interrupted }: { item: StepItem; interrupted: boolean }) {
  const [open, setOpen] = useState(false);
  const kind = item.display?.kind;
  const failure = kind === "error" ? String(item.display?.message ?? "") : "";
  const summary = stepSummary(item, interrupted, failure);
  return <>
    <button type="button" className="thought-step" aria-expanded={open} title={open ? "收起详情" : "查看详情"} onClick={() => setOpen(!open)}>
      <span>{TOOL_LABELS[item.name] ?? item.name}</span>
      {summary && <span className="thought-step__summary">{summary}</span>}
    </button>
    <StepImages item={item} />
    {failure && <p className="thought-step__error">{failure}</p>}
    {open && <div className="thought-step__detail">
      <span>参数</span><pre>{pretty(item.args)}</pre>
      {item.output !== null && <><span>结果</span><pre>{pretty(item.output)}</pre></>}
    </div>}
  </>;
}

function CopyAction({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try { await navigator.clipboard.writeText(text); setCopied(true); setTimeout(() => setCopied(false), 1500); }
    catch { notify("复制失败", "error"); }
  };
  return <button type="button" className="msg-action" title={copied ? "已复制" : "复制"} aria-label={copied ? "已复制" : "复制"} onClick={() => void copy()}>{copied ? <Check className="icon" /> : <Copy className="icon" />}</button>;
}

/** Scroll area that follows new content until the user scrolls away. */
function ChatScroll({ children }: { children: ReactNode }) {
  const { scrollRef, contentRef, isAtBottom, scrollToBottom } = useStickToBottom({ initial: "instant", resize: "smooth" });
  return <div className="chat-scroll-box">
    <div className="chat-scroll" ref={scrollRef} role="log"><div className="chat-thread" ref={contentRef}>{children}</div></div>
    {!isAtBottom && <button type="button" className="chat-jump" aria-label="回到最新消息" title="回到最新消息" onClick={() => void scrollToBottom()}><ArrowDown className="icon" /></button>}
  </div>;
}

function readCollapsed(): boolean {
  try { return localStorage.getItem(SIDEBAR_KEY) === "1"; } catch { return false; }
}

export function AgentWindow() {
  const [state, setState] = useState<AgentState | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [current, setCurrent] = useState<string | null>(null);
  const [page, setPage] = useState<"chat" | "operations">("chat");
  const [messages, setMessages] = useState<AgentMessage[]>([]);
  /** Conversation whose messages are on screen; the thread mounts only once they are, so it opens at the bottom. */
  const [loaded, setLoaded] = useState<string | null>(null);
  const [operations, setOperations] = useState<AgentOperation[]>([]);
  const [live, setLive] = useState<Live>(NO_LIVE);
  const liveRef = useRef<Live>(NO_LIVE);
  const [input, setInput] = useState("");
  const [files, setFiles] = useState<File[]>([]);
  const fileInput = useRef<HTMLInputElement>(null);
  const [dragging, setDragging] = useState(false);
  const dragDepth = useRef(0);
  const addFiles = (incoming: FileList | File[]) => {
    if (sending || !incoming.length) return;
    const next = [...files, ...Array.from(incoming)];
    try { validateFiles(next); setFiles(next); inputRef.current?.focus(); }
    catch (error) { notify(errorText(error), "error"); }
  };
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const [sending, setSending] = useState(false);
  const [collapsed, setCollapsed] = useState(readCollapsed);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [renaming, setRenaming] = useState<{ id: string; title: string } | null>(null);
  const currentRef = useRef<string | null>(null);
  const stateRequest = useRef(0);
  currentRef.current = current;

  const updateLive = useCallback((change: (previous: Live) => Live) => { liveRef.current = change(liveRef.current); setLive(liveRef.current); }, []);
  const reloadState = useCallback(async () => {
    const request = ++stateRequest.current;
    try {
      const next = await agentState();
      if (request !== stateRequest.current) return false;
      setState(next);
      setLoadError(null);
      return true;
    } catch (error) {
      if (request === stateRequest.current) setLoadError(errorText(error));
      return false;
    }
  }, []);
  const reloadOperations = useCallback(async () => {
    try { setOperations(await agentOperations(100)); } catch { /* shown on the page when it matters */ }
  }, []);
  const reloadMessages = useCallback(async (id: string | null) => {
    if (!id) { setMessages([]); setLoaded(null); return; }
    try { const list = await agentMessages(id); if (currentRef.current === id) { setMessages(list); setLoaded(id); } }
    catch (error) { notify(`读取对话失败：${errorText(error)}`, "error"); }
  }, []);
  /** Saved messages replace what was streamed so far; the streamed copy stays up until they have loaded. */
  const settleLive = useCallback((id: string) => {
    const shown = liveRef.current;
    void reloadMessages(id).then(() => {
      if (currentRef.current === id) updateLive(previous => ({ text: previous.text.slice(shown.text.length), reasoning: previous.reasoning.slice(shown.reasoning.length), tool: previous.tool }));
    });
  }, [reloadMessages, updateLive]);

  useEffect(() => {
    void reloadState(); void reloadOperations();
    void emit("agent://request-main-view").catch(() => {});
    const stops: Promise<() => void>[] = [
      listen<AgentEvent>("agent://event", event => {
        const payload = event.payload;
        if (payload.kind === "run-start" || payload.kind === "run-end") void reloadState();
        if (payload.conversationId !== currentRef.current) return;
        switch (payload.kind) {
          case "text-delta": updateLive(previous => ({ ...previous, text: previous.text + (payload.delta ?? "") })); break;
          case "reasoning-delta": updateLive(previous => ({ ...previous, reasoning: previous.reasoning + (payload.delta ?? "") })); break;
          case "tool-start": updateLive(previous => ({ ...previous, tool: payload.name ?? null })); break;
          case "tool-end": updateLive(previous => ({ ...previous, tool: null })); break;
          case "messages-changed": case "run-end": settleLive(payload.conversationId); break;
        }
      }),
      listen("agent://operations-changed", () => void reloadOperations()),
      listen("main://library-reset", () => { thumbnails.clear(); clearProgressiveImages(); setSettingsOpen(false); setState(null); setCurrent(null); setMessages([]); void reloadState(); void reloadOperations(); }),
    ];
    return () => { stops.forEach(stop => void stop.then(fn => fn()).catch(() => {})); };
  }, [reloadOperations, reloadState, settleLive, updateLive]);

  useEffect(() => { setFiles([]); setDragging(false); dragDepth.current = 0; updateLive(() => NO_LIVE); void reloadMessages(current); }, [current, reloadMessages, updateLive]);
  useEffect(() => {
    // Outside the chat pane, reject file drops instead of navigating the WebView to a local file.
    const preventNavigation = (e: DragEvent) => { if (e.dataTransfer?.types.includes("Files")) e.preventDefault(); };
    window.addEventListener("dragover", preventNavigation);
    window.addEventListener("drop", preventNavigation);
    return () => { window.removeEventListener("dragover", preventNavigation); window.removeEventListener("drop", preventNavigation); };
  }, []);

  const running = !!current && !!state?.running.includes(current);
  const items = useMemo(() => toItems(messages), [messages]);
  const thinking = running && !live.text;
  const blocks = useMemo(() => toBlocks(items, thinking), [items, thinking]);
  const appliedByPreview = useMemo(() => {
    const map = new Map<string, AgentOperation>();
    for (const operation of operations) if (operation.report?.previewId) map.set(operation.report.previewId, operation);
    return map;
  }, [operations]);
  const operationById = useMemo(() => new Map(operations.map(operation => [operation.id, operation])), [operations]);
  /** Previews already on screen in this conversation; their operations update those cards instead of adding new ones. */
  const shownPreviews = useMemo(() => new Set(items.flatMap(item => item.type === "step" && item.display?.kind === "preview" ? [String((item.display.report as AgentPreviewReport).previewId)] : [])), [items]);
  const channel = state?.channels.find(item => item.id === state.preferences.channelId) ?? state?.channels[0];
  const modelName = state && channel ? selectedAgentModel(state.preferences, channel.id) || channel.model.trim() : "";
  const openSettings = async () => {
    if (await reloadState()) setSettingsOpen(true);
  };
  const conversation = state?.conversations.find(item => item.id === current);

  const send = async (text = input) => {
    const value = text.trim();
    if ((!value && !files.length) || sending || running) return;
    if (!modelName) { void openSettings(); notify("请先选择助手使用的模型", "error"); return; }
    setSending(true);
    try {
      const attachments = await Promise.all(files.map(encodeAttachment));
      const id = await agentSend(current, value, attachments);
      if (currentRef.current !== current) { await reloadState(); return; }
      setFiles([]);
      setInput("");
      if (id !== current) setCurrent(id);
      else void reloadMessages(id);
      await reloadState();
    } catch (error) { notify(errorText(error), "error"); }
    finally { setSending(false); }
  };
  const remove = async (id: string) => {
    try { await deleteAgentConversation(id); if (current === id) setCurrent(null); await reloadState(); }
    catch (error) { notify(errorText(error), "error"); }
  };
  const rename = async () => {
    if (!renaming) return;
    try { await renameAgentConversation(renaming.id, renaming.title); setRenaming(null); await reloadState(); }
    catch (error) { notify(errorText(error), "error"); }
  };
  const newChat = () => { setFiles([]); setCurrent(null); setPage("chat"); inputRef.current?.focus(); };
  const toggleSidebar = () => {
    const next = !collapsed;
    setCollapsed(next);
    try { localStorage.setItem(SIDEBAR_KEY, next ? "1" : "0"); } catch { /* Preference only. */ }
  };

  const onCardChanged = () => { void reloadOperations(); if (current) void reloadMessages(current); };
  const renderThought = (block: ThoughtBlock, last: boolean) => {
    const working = last && thinking;
    const steps: ThoughtStep[] = block.entries.map(entry => {
      if (entry.type === "reasoning") return { key: `reasoning-${entry.id}`, state: "note", content: <Markdown className="thought-note" text={entry.text} /> };
      const pending = entry.output === null;
      return { key: `step-${entry.id}`, state: pending ? (running ? "active" : "failed") : entry.ok ? "done" : "failed", content: <StepRow item={entry} interrupted={pending && !running} /> };
    });
    if (last && running && live.reasoning) steps.push({ key: "live", state: working ? "active" : "note", content: <Markdown className="thought-note" text={live.reasoning} /> });
    if (!working && !steps.length) return null;
    const calls = block.entries.filter((entry): entry is StepItem => entry.type === "step");
    const doneLabel = calls.some(step => step.output === null) ? "已中断" : calls.length ? `已完成 ${calls.length} 步` : "已思考";
    return <Fragment key={block.key}>
      <div className="msg msg-thought"><ThoughtLine working={working} startedAt={block.startedAt} endedAt={block.endedAt} steps={steps} doneLabel={doneLabel} /></div>
      {calls.map(step => {
        const display = step.display;
        if (display?.kind === "images") return <ViewedImages key={step.id} item={step} />;
        if (display?.kind === "preview") {
          const report = display.report as AgentPreviewReport;
          const conversationId = current;
          const onApply = conversationId && !running ? async () => {
            await applyAgentPreview(conversationId, report.previewId);
            await reloadOperations();
            void reloadMessages(conversationId);
          } : undefined;
          return <div key={step.id} className="msg msg-card"><PreviewCard report={report} applied={appliedByPreview.get(report.previewId)} onApply={onApply} onChanged={onCardChanged} /></div>;
        }
        if (display?.kind === "operation") {
          const stored = display.operation as AgentOperation;
          const previewId = (stored.report as Partial<AgentPreviewReport> | undefined)?.previewId;
          if (previewId && shownPreviews.has(previewId)) return null;
          return <div key={step.id} className="msg msg-card"><OperationCard operation={operationById.get(stored.id) ?? stored} onChanged={onCardChanged} /></div>;
        }
        return null;
      })}
    </Fragment>;
  };
  const renderBlock = (block: Block, index: number) => {
    switch (block.type) {
      case "user": return <div key={block.id} className="msg msg-user">
        {block.files.length > 0 && <div className="ag-file-list">{block.files.map((name, i) => <span className="ag-file-chip" key={i} title={name}><FileText className="icon sm" /><span>{name}</span></span>)}</div>}
        {block.text && <p className="msg-bubble">{block.text}</p>}
      </div>;
      case "assistant": return <div key={block.id} className="msg msg-assistant">
        <Markdown text={block.text} />
        <div className="msg-actions"><CopyAction text={block.text} /></div>
      </div>;
      case "action": return <p key={block.id} className="msg msg-user-action"><Check className="icon sm" aria-hidden="true" /><span>{block.action === "undo" ? "你撤销了" : "你确认执行了"}「{block.title}」</span>{block.operationId !== null && <span className="ag-time">#{block.operationId}</span>}</p>;
      case "error": return block.stopped
        ? <p key={block.id} className="msg msg-notice">{block.text}</p>
        : <p key={block.id} className="msg msg-error" role="alert">{block.text}</p>;
      case "thought": return renderThought(block, index === blocks.length - 1);
    }
  };

  const empty = page === "chat" && !current && !loadError;
  const title = loadError ? "整理助手" : page === "operations" ? "操作记录" : conversation?.title || "新对话";
  return <Tooltip.Provider delayDuration={350}><ImageViewerProvider>
    <div className={`chat-view${collapsed ? " is-collapsed" : ""}`} onContextMenu={event => { if (!(event.target instanceof Element && event.target.closest("input, textarea, .msg, .ag-card"))) event.preventDefault(); }}>
      <aside className="chat-side" aria-label="历史对话" inert={collapsed}>
        <div className="chat-side-head" data-tauri-drag-region>
          <button type="button" className="chat-new-btn" onClick={newChat}><SquarePen className="icon" /><span>新对话</span></button>
          <button type="button" className="icon-btn" title="收起侧栏" aria-label="收起侧栏" onClick={toggleSidebar}><PanelLeft className="icon" /></button>
        </div>
        <div className="chat-side-nav">
          <button type="button" className={`chat-item${page === "operations" ? " active" : ""}`} onClick={() => { setPage("operations"); void reloadOperations(); }}><History className="icon" /><span className="chat-item-title">操作记录</span></button>
        </div>
        <p className="chat-side-label" data-tauri-drag-region>最近对话</p>
        <nav className="chat-list" aria-label="对话列表">
          {state?.conversations.map(item => <div key={item.id} className={`chat-item has-menu${page === "chat" && current === item.id ? " active" : ""}`}>
            <button type="button" className="chat-item-main" title={`${item.title}\n${formatTime(item.updatedAt)}`} aria-current={page === "chat" && current === item.id ? "page" : undefined} onClick={() => { setCurrent(item.id); setPage("chat"); }}>
              <span className="chat-item-title">{item.title}</span>
              {state.running.includes(item.id) && <span className="chat-item-busy" role="img" aria-label="正在处理" />}
            </button>
            <Menu label="更多" trigger={<button type="button" className="chat-item-more" aria-label="更多操作"><MoreHorizontal className="icon" /></button>} items={[
              { label: "重命名", action: () => setRenaming({ id: item.id, title: item.title }) },
              { label: "删除对话", hint: "不会撤销已做的修改", action: () => void remove(item.id) },
            ]} />
          </div>)}
          {state && state.conversations.length === 0 && <p className="chat-side-empty">还没有对话</p>}
        </nav>
        <p className="chat-side-note">改动前会先预演给你看；每次修改都能在「操作记录」里撤销，原图文件不会被改动。</p>
      </aside>

      <div className={`chat-main${empty ? " is-empty" : ""}`}
        onDragEnter={event => { if (page === "chat" && !loadError && !sending && event.dataTransfer.types.includes("Files")) { event.preventDefault(); dragDepth.current++; setDragging(true); } }}
        onDragLeave={() => { dragDepth.current = Math.max(0, dragDepth.current - 1); if (!dragDepth.current) setDragging(false); }}
        onDragOver={event => { if (event.dataTransfer.types.includes("Files")) { event.preventDefault(); event.dataTransfer.dropEffect = page === "chat" && !sending && !loadError ? "copy" : "none"; } }}
        onDrop={event => { if (!event.dataTransfer.types.includes("Files")) return; event.preventDefault(); dragDepth.current = 0; setDragging(false); if (page === "chat" && !loadError) addFiles(event.dataTransfer.files); }}>
        {dragging && <div className="ag-drop-target"><Paperclip className="icon" />松开以添加附件</div>}
        <header className="chat-head" data-tauri-drag-region>
          {collapsed && <>
            <button type="button" className="icon-btn" title="展开侧栏" aria-label="展开侧栏" onClick={toggleSidebar}><PanelLeft className="icon" /></button>
            <button type="button" className="icon-btn" title="新对话" aria-label="新对话" onClick={newChat}><SquarePen className="icon" /></button>
          </>}
          <h1 className="chat-title" data-tauri-drag-region>{title}</h1>
          <WindowControls />
        </header>

        {loadError ? <div className="chat-fail"><h2>整理助手暂时不可用</h2><p>{loadError}</p><Button onClick={() => void reloadState()}>重试</Button></div>
          : page === "operations" ? <div className="chat-scroll"><div className="chat-thread is-list">
            <p className="chat-lede">保留最近 100 次修改。关掉窗口或重启后也能撤销；撤销时，之后又被改过的图片会保留现状。</p>
            {operations.length === 0 ? <p className="ag-empty">还没有执行过修改。</p> : operations.map(operation => <OperationCard key={operation.id} operation={operation} onChanged={() => void reloadOperations()} />)}
          </div></div>
            : <>
              {current && loaded !== current ? <div className="chat-scroll-box" /> : <ChatScroll key={current ?? "new"}>
                {!current && <div className="chat-hero">
                  <span className="chat-hero-mark" aria-hidden="true"><Sparkles className="icon lg" /></span>
                  <h2>想怎么整理图片？</h2>
                  <p>用大白话告诉我就行。改数据之前我会先预演给你看，有不确定的地方会先问你；每次修改都能撤销，原图文件不会被改动。</p>
                </div>}
                {blocks.map(renderBlock)}
                {running && live.text && <div className="msg msg-assistant"><Markdown text={live.text} streaming /></div>}
              </ChatScroll>}
              <div className="chat-dock">
                <form className="chat-composer" onSubmit={event => { event.preventDefault(); void send(); }} onClick={event => { if (!(event.target instanceof Element && event.target.closest("button"))) inputRef.current?.focus(); }}>
                  <input ref={fileInput} type="file" hidden multiple accept={attachmentAccept} onChange={event => { if (event.currentTarget.files) addFiles(event.currentTarget.files); event.currentTarget.value = ""; }} />
                  {files.length > 0 && <><div className="ag-file-list">{files.map((file, i) => <DraftAttachment key={`${file.name}-${file.lastModified}-${file.size}-${i}`} file={file} disabled={sending} onRemove={() => setFiles(old => old.filter((_, at) => at !== i))} />)}</div><small className="ag-file-note">点击发送后，附件内容将发送给当前模型。</small></>}
                  <textarea ref={inputRef} readOnly={sending} onPaste={event => { if (event.clipboardData.files.length) { event.preventDefault(); addFiles(event.clipboardData.files); } }} rows={1} value={input} autoFocus aria-label="给整理助手发消息" placeholder={running ? "助手正在处理…" : "描述你想怎么整理图片"}
                    onChange={event => setInput(event.target.value)}
                    onKeyDown={event => {
                      // Enter that confirms an IME candidate must not send.
                      if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing || event.keyCode === 229) return;
                      event.preventDefault();
                      void send();
                    }} />
                  <div className="composer-bar">
                    <button type="button" className="icon-btn" aria-label="添加图片或文件" title="添加图片、PDF 或文本；也可拖入或粘贴截图" disabled={sending || files.length >= 4} onClick={() => fileInput.current?.click()}><Paperclip className="icon" /></button>
                    <button type="button" className="chip" title="选择助手使用的模型和思考强度" onClick={() => void openSettings()}>
                      <Sparkles className="icon sm" />
                      <span className="chip-text">{modelName || "选择模型"}</span>
                      {modelName && <span className="chip-auto">{EFFORT_SHORT[state?.preferences.effort || "medium"] ?? state?.preferences.effort}</span>}
                      <ChevronDown className="icon sm chip-caret" />
                    </button>
                    <span className="composer-spacer" />
                    {running
                      ? <button type="button" className="composer-send is-stop" aria-label="停止" title="停止（已经完成的修改会保留）" onClick={() => { if (current) void agentCancel(current); }}><svg className="icon" viewBox="0 0 24 24" aria-hidden="true"><rect width="12" height="12" x="6" y="6" rx="2" fill="currentColor" stroke="none" /></svg></button>
                      : <button type="submit" className="composer-send" aria-label="发送" title="发送（Enter）" disabled={(!input.trim() && !files.length) || sending}>{sending ? <span className="spinner" aria-hidden="true" /> : <ArrowUp className="icon" />}</button>}
                  </div>
                </form>
                {!current && <div className="chat-suggestions" role="group" aria-label="开场示例">
                  {SUGGESTIONS.map(item => <button key={item.label} type="button" className="suggestion" onClick={() => { setInput(item.text); inputRef.current?.focus(); }}>{item.label}</button>)}
                </div>}
                <p className="chat-hint">Enter 发送 · Shift+Enter 换行 · 每次修改都能在「操作记录」里撤销</p>
              </div>
            </>}
      </div>

      {state && <AgentPreferencesDialog key={state.libraryId} state={state} open={settingsOpen} onClose={() => setSettingsOpen(false)}
        onSaved={preferences => {
          setState(current => current?.libraryId === state.libraryId ? { ...current, preferences } : current);
          void reloadState();
        }} />}
      <Modal open={!!renaming} onClose={() => setRenaming(null)} title="重命名对话" footer={<><Button onClick={() => setRenaming(null)}>取消</Button><Button variant="primary" onClick={() => void rename()}>保存</Button></>}>
        <input className="r-input ag-full" value={renaming?.title ?? ""} onChange={event => renaming && setRenaming({ ...renaming, title: event.target.value })} onKeyDown={event => { if (event.key === "Enter") void rename(); }} />
      </Modal>
      <Notices />
    </div>
  </ImageViewerProvider></Tooltip.Provider>;
}
