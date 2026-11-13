import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { ArrowUp, Bot, Check, ChevronDown, ChevronRight, History, Loader2, MessageSquarePlus, MoreHorizontal, Settings2, Square, X } from "lucide-react";
import {
  agentCancel, agentMessages, agentModels, agentOperations, agentSend, agentState, deleteAgentConversation, renameAgentConversation, saveAgentPreferences,
  type AgentEvent, type AgentMessage, type AgentOperation, type AgentPreferences, type AgentPreviewReport, type AgentState,
} from "../../../lib/api/agent";
import { errorText } from "../../../lib/utils/format";
import { Button, Menu, Modal, Select, Tooltip } from "../../ui/controls";
import { Notices } from "../../ui/Notices";
import { WindowControls } from "../../ui/WindowControls";
import { notify } from "../../state/notices";
import { OperationCard, PreviewCard } from "./AgentCards";
import { RichText, TOOL_LABELS, formatTime } from "./agent-format";
import "./agent.css";

const EFFORTS = [["minimal", "最快（minimal）"], ["low", "低（low）"], ["medium", "中（medium，推荐）"], ["high", "高（high，更慢）"], ["default", "模型默认"]] as const;
const SUGGESTIONS = [
  "把我选中图片的正向提示词里「Text:」以及后面的内容删掉",
  "给某个画师串的所有图都加一个 TAG",
  "在主窗口里同时选中几个画师串的所有图",
  "现在图库里有多少张没有分组的图？",
];

type Live = { text: string; reasoning: string; tool: string | null };
type Item =
  | { type: "user"; id: number; text: string }
  | { type: "assistant"; id: number; text: string }
  | { type: "error"; id: number; text: string; stopped: boolean }
  | { type: "step"; id: number; name: string; args: string; output: string | null; ok: boolean; display: Record<string, unknown> | null };

function toItems(messages: AgentMessage[]): Item[] {
  const items: Item[] = [];
  const steps = new Map<string, Extract<Item, { type: "step" }>>();
  for (const message of messages) {
    const content = message.content;
    if (message.kind === "user") items.push({ type: "user", id: message.id, text: String(content.text ?? "") });
    else if (message.kind === "assistant") items.push({ type: "assistant", id: message.id, text: String(content.text ?? "") });
    else if (message.kind === "error") items.push({ type: "error", id: message.id, text: String(content.message ?? ""), stopped: content.stopped === true });
    else if (message.kind === "tool_call") {
      const step: Extract<Item, { type: "step" }> = { type: "step", id: message.id, name: String(content.name ?? ""), args: String(content.arguments ?? ""), output: null, ok: true, display: null };
      steps.set(String(content.callId), step); items.push(step);
    } else if (message.kind === "tool_result") {
      const step = steps.get(String(content.callId));
      if (step) { step.output = String(content.output ?? ""); step.ok = content.ok !== false; step.display = (content.display as Record<string, unknown>) ?? null; }
    }
  }
  return items;
}

function pretty(text: string): string {
  try { return JSON.stringify(JSON.parse(text), null, 2); } catch { return text; }
}

function Step({ item, running }: { item: Extract<Item, { type: "step" }>; running: boolean }) {
  const [open, setOpen] = useState(false);
  const pending = item.output === null;
  const label = TOOL_LABELS[item.name] ?? item.name;
  const kind = item.display?.kind;
  const summary = kind === "selection" ? `已选中 ${item.display?.count} 张` : kind === "error" ? String(item.display?.message ?? "") : "";
  return <div className={`ag-step${item.ok ? "" : " is-failed"}`}>
    <button type="button" className="ag-step-head" onClick={() => setOpen(!open)}>
      {pending ? (running ? <Loader2 size={13} className="ag-spin" /> : <X size={13} />) : item.ok ? <Check size={13} /> : <X size={13} />}
      <span>{label}</span>
      {summary && <span className="ag-step-summary">{summary}</span>}
      {open ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
    </button>
    {open && <div className="ag-step-body">
      <h6>参数</h6><pre>{pretty(item.args)}</pre>
      {item.output !== null && <><h6>结果</h6><pre>{pretty(item.output)}</pre></>}
      {pending && !running && <p className="ag-hint">这一步被中断，没有执行。</p>}
    </div>}
  </div>;
}

function PreferencesDialog({ state, open, onClose, onSaved }: { state: AgentState; open: boolean; onClose: () => void; onSaved: () => void }) {
  const [draft, setDraft] = useState<AgentPreferences>(state.preferences);
  const [models, setModels] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const channelId = draft.channelId || state.channels[0]?.id || "";
  useEffect(() => { if (open) setDraft(state.preferences); }, [open, state.preferences]);
  useEffect(() => {
    if (!open || !channelId) return;
    let stale = false;
    setLoading(true);
    agentModels(channelId).then(list => { if (!stale) setModels(list); }).catch(error => { if (!stale) { setModels([]); notify(`获取模型列表失败：${errorText(error)}`, "error"); } }).finally(() => { if (!stale) setLoading(false); });
    return () => { stale = true; };
  }, [open, channelId]);
  const save = async () => {
    try { await saveAgentPreferences({ ...draft, channelId }); onSaved(); onClose(); }
    catch (error) { notify(`保存失败：${errorText(error)}`, "error"); }
  };
  return <Modal open={open} onClose={onClose} title="助手使用的模型" description="渠道和 Key 沿用工具箱「模型设置」里的配置。助手需要支持 Responses 接口和工具调用的模型。" width={480}
    footer={<><Button variant="ghost" onClick={onClose}>取消</Button><Button variant="primary" onClick={() => void save()}>保存</Button></>}>
    {state.channels.length === 0 ? <p className="ag-hint">还没有模型渠道，请先到工具箱的「模型设置」里添加。</p> : <div className="ag-form">
      <label>渠道<Select label="渠道" value={channelId} onChange={value => setDraft({ ...draft, channelId: value, model: "" })} options={state.channels.map(channel => [channel.id, `${channel.name}${channel.hasKey ? "" : "（没有 Key）"}`] as const)} /></label>
      <label>模型
        <input className="r-input" list="ag-models" value={draft.model} placeholder={loading ? "正在获取模型列表…" : "输入或选择模型"} onChange={event => setDraft({ ...draft, model: event.target.value })} />
        <datalist id="ag-models">{models.map(model => <option key={model} value={model} />)}</datalist>
      </label>
      <label>思考强度<Select label="思考强度" value={draft.effort || "medium"} onChange={value => setDraft({ ...draft, effort: value })} options={EFFORTS} /></label>
      <p className="ag-hint">思考强度越高越稳，但每一步更慢。模型不支持思考强度时请选「模型默认」。</p>
    </div>}
  </Modal>;
}

function OperationsPage({ operations, reload }: { operations: AgentOperation[]; reload: () => void }) {
  return <div className="ag-scroll"><div className="ag-column">
    <h2 className="ag-page-title">操作记录</h2>
    <p className="ag-hint">保留最近 100 次修改。关掉窗口或重启后也能撤销；撤销时，之后又被改过的图片会保留现状。</p>
    {operations.length === 0 ? <p className="ag-empty">还没有执行过修改。</p> : operations.map(operation => <OperationCard key={operation.id} operation={operation} onChanged={reload} compact />)}
  </div></div>;
}

export function AgentWindow() {
  const [state, setState] = useState<AgentState | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [current, setCurrent] = useState<string | null>(null);
  const [page, setPage] = useState<"chat" | "operations">("chat");
  const [messages, setMessages] = useState<AgentMessage[]>([]);
  const [operations, setOperations] = useState<AgentOperation[]>([]);
  const [live, setLive] = useState<Live>({ text: "", reasoning: "", tool: null });
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [renaming, setRenaming] = useState<{ id: string; title: string } | null>(null);
  const currentRef = useRef<string | null>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  currentRef.current = current;

  const reloadState = useCallback(async () => {
    try { setState(await agentState()); setLoadError(null); } catch (error) { setLoadError(errorText(error)); }
  }, []);
  const reloadOperations = useCallback(async () => {
    try { setOperations(await agentOperations(100)); } catch { /* shown on the page when it matters */ }
  }, []);
  const reloadMessages = useCallback(async (id: string | null) => {
    if (!id) { setMessages([]); return; }
    try { const list = await agentMessages(id); if (currentRef.current === id) setMessages(list); }
    catch (error) { notify(`读取对话失败：${errorText(error)}`, "error"); }
  }, []);

  useEffect(() => {
    void reloadState(); void reloadOperations();
    void emit("agent://request-main-view").catch(() => {});
    const stops: Promise<() => void>[] = [
      listen<AgentEvent>("agent://event", event => {
        const payload = event.payload;
        if (payload.kind === "run-start" || payload.kind === "run-end") void reloadState();
        if (payload.conversationId !== currentRef.current) return;
        switch (payload.kind) {
          case "text-delta": setLive(previous => ({ ...previous, text: previous.text + (payload.delta ?? "") })); break;
          case "reasoning-delta": setLive(previous => ({ ...previous, reasoning: previous.reasoning + (payload.delta ?? "") })); break;
          case "tool-start": setLive(previous => ({ ...previous, tool: payload.name ?? null })); break;
          case "tool-end": setLive(previous => ({ ...previous, tool: null })); break;
          case "messages-changed": setLive({ text: "", reasoning: "", tool: null }); void reloadMessages(payload.conversationId); break;
          case "run-end": setLive({ text: "", reasoning: "", tool: null }); void reloadMessages(payload.conversationId); break;
        }
      }),
      listen("agent://operations-changed", () => void reloadOperations()),
      listen("main://library-reset", () => { setCurrent(null); setMessages([]); void reloadState(); void reloadOperations(); }),
    ];
    return () => { stops.forEach(stop => void stop.then(fn => fn()).catch(() => {})); };
  }, [reloadMessages, reloadOperations, reloadState]);

  useEffect(() => { setLive({ text: "", reasoning: "", tool: null }); stick.current = true; void reloadMessages(current); }, [current, reloadMessages]);
  useEffect(() => {
    const element = scroller.current;
    if (element && stick.current) element.scrollTop = element.scrollHeight;
  }, [messages, live]);

  const running = !!current && !!state?.running.includes(current);
  const items = useMemo(() => toItems(messages), [messages]);
  const appliedByPreview = useMemo(() => {
    const map = new Map<string, AgentOperation>();
    for (const operation of operations) if (operation.report?.previewId) map.set(operation.report.previewId, operation);
    return map;
  }, [operations]);
  const operationById = useMemo(() => new Map(operations.map(operation => [operation.id, operation])), [operations]);
  const channel = state?.channels.find(item => item.id === state.preferences.channelId) ?? state?.channels[0];
  const modelName = state?.preferences.model || channel?.model || "";

  const send = async (text = input) => {
    const value = text.trim();
    if (!value || sending || running) return;
    if (!modelName) { setSettingsOpen(true); notify("请先选择助手使用的模型", "error"); return; }
    setSending(true);
    try {
      const id = await agentSend(current, value);
      setInput("");
      stick.current = true;
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

  const renderItem = (item: Item) => {
    switch (item.type) {
      case "user": return <div key={item.id} className="ag-msg is-user"><div className="ag-bubble">{item.text}</div></div>;
      case "assistant": return <div key={item.id} className="ag-msg is-assistant"><RichText text={item.text} /></div>;
      case "error": return <div key={item.id} className={`ag-msg ag-error${item.stopped ? " is-stopped" : ""}`}>{item.text}</div>;
      case "step": {
        const display = item.display;
        let card = null;
        if (display?.kind === "preview") {
          const report = display.report as AgentPreviewReport;
          card = <PreviewCard report={report} applied={appliedByPreview.get(report.previewId)} />;
        } else if (display?.kind === "operation") {
          const stored = display.operation as AgentOperation;
          card = <OperationCard operation={operationById.get(stored.id) ?? stored} onChanged={() => void reloadOperations()} />;
        }
        return <div key={item.id} className="ag-msg is-step"><Step item={item} running={running} />{card}</div>;
      }
    }
  };

  return <Tooltip.Provider delayDuration={350}>
    <div className="ag-window" onContextMenu={event => { if (!(event.target instanceof Element && event.target.closest("input, textarea, pre, .ag-rich, .ag-bubble, .ag-card"))) event.preventDefault(); }}>
      <header className="titlebar" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region><Bot size={17} /><strong data-tauri-drag-region>整理助手</strong></div>
        <div className="ag-titlebar-tools">
          <button type="button" className="ag-model-button" onClick={() => setSettingsOpen(true)} title="选择助手使用的模型">
            <Settings2 size={14} />{modelName ? `${modelName} · ${state?.preferences.effort || "medium"}` : "选择模型"}
          </button>
          <WindowControls />
        </div>
      </header>
      <div className="ag-body">
        <aside className="ag-sidebar">
          <Button className="ag-new" onClick={() => { setCurrent(null); setPage("chat"); }}><MessageSquarePlus size={15} />新对话</Button>
          <button type="button" className={`ag-nav${page === "operations" ? " is-active" : ""}`} onClick={() => { setPage("operations"); void reloadOperations(); }}><History size={15} />操作记录</button>
          <div className="ag-sidebar-label">对话</div>
          <div className="ag-conversations">
            {state?.conversations.map(conversation => <div key={conversation.id} className={`ag-conversation${page === "chat" && current === conversation.id ? " is-active" : ""}`}>
              <button type="button" className="ag-conversation-main" onClick={() => { setCurrent(conversation.id); setPage("chat"); }}>
                <span className="ag-conversation-title">{state.running.includes(conversation.id) && <Loader2 size={12} className="ag-spin" />}{conversation.title}</span>
                <span className="ag-time">{formatTime(conversation.updatedAt)}</span>
              </button>
              <Menu label="更多" trigger={<button type="button" className="ag-icon" aria-label="更多操作"><MoreHorizontal size={14} /></button>} items={[
                { label: "重命名", action: () => setRenaming({ id: conversation.id, title: conversation.title }) },
                { label: "删除对话", hint: "不会撤销已做的修改", action: () => void remove(conversation.id) },
              ]} />
            </div>)}
            {state && state.conversations.length === 0 && <p className="ag-hint ag-pad">还没有对话</p>}
          </div>
        </aside>
        <main className="ag-main">
          {loadError ? <div className="ag-state"><h1>整理助手暂时不可用</h1><p>{loadError}</p><Button onClick={() => void reloadState()}>重试</Button></div>
            : page === "operations" ? <OperationsPage operations={operations} reload={() => void reloadOperations()} />
              : <>
                <div className="ag-scroll" ref={scroller} onScroll={event => { const element = event.currentTarget; stick.current = element.scrollHeight - element.scrollTop - element.clientHeight < 80; }}>
                  <div className="ag-column">
                    {!current && messages.length === 0 && <div className="ag-welcome">
                      <Bot size={30} />
                      <h1>想怎么整理图片？</h1>
                      <p>用大白话告诉我就行。改数据之前我会先预演给你看，有不确定的地方会先问你；每次修改都能撤销，原图文件不会被改动。</p>
                      <div className="ag-suggestions">{SUGGESTIONS.map(text => <button key={text} type="button" onClick={() => setInput(text)}>{text}</button>)}</div>
                    </div>}
                    {items.map(renderItem)}
                    {running && <div className="ag-msg is-assistant ag-live">
                      {live.reasoning && !live.text && <details className="ag-reasoning"><summary>思考中…</summary><p>{live.reasoning}</p></details>}
                      {live.text ? <RichText text={live.text} /> : <p className="ag-thinking"><Loader2 size={14} className="ag-spin" />{live.tool ? `正在${TOOL_LABELS[live.tool] ?? live.tool}…` : "思考中…"}</p>}
                    </div>}
                  </div>
                </div>
                <div className="ag-composer-wrap"><div className="ag-composer">
                  <textarea value={input} rows={1} placeholder={running ? "助手正在处理…" : "描述你想怎么整理图片，Enter 发送，Shift+Enter 换行"}
                    onChange={event => setInput(event.target.value)}
                    onKeyDown={event => { if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); void send(); } }} />
                  {running
                    ? <Button variant="default" className="ag-send" aria-label="停止" title="停止" onClick={() => current && void agentCancel(current)}><Square size={13} /></Button>
                    : <Button variant="primary" className="ag-send" aria-label="发送" title="发送" disabled={!input.trim() || sending} onClick={() => void send()}><ArrowUp size={16} /></Button>}
                </div></div>
              </>}
        </main>
      </div>
      {state && <PreferencesDialog state={state} open={settingsOpen} onClose={() => setSettingsOpen(false)} onSaved={() => void reloadState()} />}
      <Modal open={!!renaming} onClose={() => setRenaming(null)} title="重命名对话" footer={<><Button variant="ghost" onClick={() => setRenaming(null)}>取消</Button><Button variant="primary" onClick={() => void rename()}>保存</Button></>}>
        <input className="r-input ag-full" value={renaming?.title ?? ""} onChange={event => renaming && setRenaming({ ...renaming, title: event.target.value })} onKeyDown={event => { if (event.key === "Enter") void rename(); }} />
      </Modal>
      <Notices />
    </div>
  </Tooltip.Provider>;
}
