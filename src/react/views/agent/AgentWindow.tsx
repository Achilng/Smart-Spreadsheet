import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { Bot, Check, Copy, History, Loader2, MessageSquarePlus, MoreHorizontal, Settings2 } from "lucide-react";
import {
  agentCancel, agentMessages, agentModels, agentOperations, agentSend, agentState, deleteAgentConversation, renameAgentConversation, saveAgentPreferences,
  type AgentEvent, type AgentMessage, type AgentOperation, type AgentPreferences, type AgentPreviewReport, type AgentState,
} from "../../../lib/api/agent";
import { errorText } from "../../../lib/utils/format";
import { Conversation, ConversationContent, ConversationScrollButton } from "../../ui/ai-elements/conversation";
import { Message, MessageAction, MessageActions, MessageContent, MessageResponse } from "../../ui/ai-elements/message";
import { PromptInput, PromptInputButton, PromptInputFooter, PromptInputSubmit, PromptInputTextarea, PromptInputTools } from "../../ui/ai-elements/prompt-input";
import { Reasoning, ReasoningContent, ReasoningTrigger } from "../../ui/ai-elements/reasoning";
import { Shimmer } from "../../ui/ai-elements/shimmer";
import { Suggestion, Suggestions } from "../../ui/ai-elements/suggestion";
import { Tool, ToolContent, ToolHeader, ToolInput, ToolOutput, type ToolState } from "../../ui/ai-elements/tool";
import { Button, Menu, Modal, Select, Tooltip } from "../../ui/controls";
import { Notices } from "../../ui/Notices";
import { WindowControls } from "../../ui/WindowControls";
import { notify } from "../../state/notices";
import { OperationCard, PreviewCard } from "./AgentCards";
import { TOOL_LABELS, formatTime } from "./agent-format";
import "./agent.css";

const EFFORTS = [["minimal", "最快（minimal）"], ["low", "低（low）"], ["medium", "中（medium，推荐）"], ["high", "高（high，更慢）"], ["default", "模型默认"]] as const;
const SUGGESTIONS = [
  "把选中图片正向提示词里「Text:」后面的内容删掉",
  "给某个画师串的所有图加一个 TAG",
  "在主窗口同时选中几个画师串的图",
  "图库里有多少张图还没分组？",
];
const NO_LIVE = { text: "", reasoning: "", tool: null };

type Live = { text: string; reasoning: string; tool: string | null };
type StepItem = { type: "step"; id: number; name: string; args: string; output: string | null; ok: boolean; display: Record<string, unknown> | null };
type Item =
  | { type: "user"; id: number; text: string }
  | { type: "assistant"; id: number; text: string }
  | { type: "reasoning"; id: number; text: string }
  | { type: "error"; id: number; text: string; stopped: boolean }
  | StepItem;

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
    const reasoning = reasoningText(content.reasoning);
    if (reasoning) items.push({ type: "reasoning", id: message.id, text: reasoning });
    if (message.kind === "user") items.push({ type: "user", id: message.id, text: String(content.text ?? "") });
    else if (message.kind === "assistant") items.push({ type: "assistant", id: message.id, text: String(content.text ?? "") });
    else if (message.kind === "error") items.push({ type: "error", id: message.id, text: String(content.message ?? ""), stopped: content.stopped === true });
    else if (message.kind === "tool_call") {
      const step: StepItem = { type: "step", id: message.id, name: String(content.name ?? ""), args: String(content.arguments ?? ""), output: null, ok: true, display: null };
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

function Step({ item, running }: { item: StepItem; running: boolean }) {
  const pending = item.output === null;
  const state: ToolState = pending ? (running ? "running" : "interrupted") : item.ok ? "done" : "error";
  const kind = item.display?.kind;
  const summary = kind === "selection" ? `已选中 ${item.display?.count} 张` : kind === "error" ? String(item.display?.message ?? "") : undefined;
  return <Tool>
    <ToolHeader title={TOOL_LABELS[item.name] ?? item.name} state={state} summary={summary} />
    <ToolContent>
      <ToolInput input={pretty(item.args)} />
      {item.ok ? <ToolOutput output={item.output === null ? null : pretty(item.output)} /> : <ToolOutput errorText={kind === "error" ? String(item.display?.message ?? "") : pretty(item.output ?? "")} />}
      {state === "interrupted" && <p className="px-3 pb-3 text-muted-foreground text-xs">这一步被中断，没有执行。</p>}
    </ToolContent>
  </Tool>;
}

function CopyAction({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try { await navigator.clipboard.writeText(text); setCopied(true); setTimeout(() => setCopied(false), 1500); }
    catch { notify("复制失败", "error"); }
  };
  return <MessageAction tooltip={copied ? "已复制" : "复制"} onClick={() => void copy()}>{copied ? <Check /> : <Copy />}</MessageAction>;
}

/** Tested on real requests: quick, and consistently asks instead of guessing. */
const RECOMMENDED_MODEL = /gpt-6-luna/i;

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
    agentModels(channelId).then(list => {
      if (stale) return;
      setModels(list);
      // Pre-fill the tested recommendation when nothing has been chosen yet.
      const recommended = list.find(name => RECOMMENDED_MODEL.test(name));
      if (recommended) setDraft(current => current.model ? current : { ...current, model: recommended });
    }).catch(error => { if (!stale) { setModels([]); notify(`获取模型列表失败：${errorText(error)}`, "error"); } }).finally(() => { if (!stale) setLoading(false); });
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
      <p className="ag-hint">推荐 gpt-6-luna + 中等思考强度：又快又谨慎。DeepSeek、Gemini 也能用，但每轮要慢不少。思考强度越高越稳，但每一步更慢；模型不支持思考强度时请选「模型默认」。</p>
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
  /** Conversation whose messages are on screen; the list mounts only once they are, so it opens at the bottom. */
  const [loaded, setLoaded] = useState<string | null>(null);
  const [operations, setOperations] = useState<AgentOperation[]>([]);
  const [live, setLive] = useState<Live>(NO_LIVE);
  const [input, setInput] = useState("");
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const [sending, setSending] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [renaming, setRenaming] = useState<{ id: string; title: string } | null>(null);
  const currentRef = useRef<string | null>(null);
  currentRef.current = current;

  const reloadState = useCallback(async () => {
    try { setState(await agentState()); setLoadError(null); } catch (error) { setLoadError(errorText(error)); }
  }, []);
  const reloadOperations = useCallback(async () => {
    try { setOperations(await agentOperations(100)); } catch { /* shown on the page when it matters */ }
  }, []);
  const reloadMessages = useCallback(async (id: string | null) => {
    if (!id) { setMessages([]); setLoaded(null); return; }
    try { const list = await agentMessages(id); if (currentRef.current === id) { setMessages(list); setLoaded(id); } }
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
          case "messages-changed": setLive(NO_LIVE); void reloadMessages(payload.conversationId); break;
          case "run-end": setLive(NO_LIVE); void reloadMessages(payload.conversationId); break;
        }
      }),
      listen("agent://operations-changed", () => void reloadOperations()),
      listen("main://library-reset", () => { setCurrent(null); setMessages([]); void reloadState(); void reloadOperations(); }),
    ];
    return () => { stops.forEach(stop => void stop.then(fn => fn()).catch(() => {})); };
  }, [reloadMessages, reloadOperations, reloadState]);

  useEffect(() => { setLive(NO_LIVE); void reloadMessages(current); }, [current, reloadMessages]);

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
  const lastItem = items[items.length - 1];
  const toolRunning = lastItem?.type === "step" && lastItem.output === null;

  const send = async (text = input) => {
    const value = text.trim();
    if (!value || sending || running) return;
    if (!modelName) { setSettingsOpen(true); notify("请先选择助手使用的模型", "error"); return; }
    setSending(true);
    try {
      const id = await agentSend(current, value);
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

  const renderItem = (item: Item) => {
    switch (item.type) {
      case "user": return <Message key={item.id} from="user" className="mt-2 first:mt-0">
        <MessageContent className="select-text whitespace-pre-wrap leading-relaxed [overflow-wrap:anywhere]">{item.text}</MessageContent>
      </Message>;
      case "assistant": return <Message key={item.id} from="assistant" className="max-w-full">
        <MessageContent className="ag-prose w-full select-text"><MessageResponse>{item.text}</MessageResponse></MessageContent>
        <MessageActions className="opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100"><CopyAction text={item.text} /></MessageActions>
      </Message>;
      case "reasoning": return <Reasoning key={`reasoning-${item.id}`} defaultOpen={false} className="select-text">
        <ReasoningTrigger /><ReasoningContent className="ag-prose">{item.text}</ReasoningContent>
      </Reasoning>;
      case "error": return item.stopped
        ? <p key={item.id} className="text-muted-foreground text-sm">{item.text}</p>
        : <div key={item.id} className="select-text whitespace-pre-wrap rounded-lg bg-destructive/10 px-4 py-3 text-destructive text-sm leading-relaxed [overflow-wrap:anywhere]">{item.text}</div>;
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
        return <div key={item.id} className="flex flex-col gap-3"><Step item={item} running={running} />{card}</div>;
      }
    }
  };

  return <Tooltip.Provider delayDuration={350}>
    <div className="ag-window ai-ui" onContextMenu={event => { if (!(event.target instanceof Element && event.target.closest("input, textarea, pre, .select-text, .ag-card"))) event.preventDefault(); }}>
      <header className="titlebar" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region><Bot size={17} /><strong data-tauri-drag-region>整理助手</strong></div>
        <WindowControls />
      </header>
      <div className="ag-body">
        <aside className="ag-sidebar">
          <button type="button" className="ag-nav" onClick={() => { setCurrent(null); setPage("chat"); }}><MessageSquarePlus size={16} />新对话</button>
          <button type="button" className={`ag-nav${page === "operations" ? " is-active" : ""}`} onClick={() => { setPage("operations"); void reloadOperations(); }}><History size={16} />操作记录</button>
          <div className="ag-sidebar-label">对话</div>
          <div className="ag-conversations">
            {state?.conversations.map(conversation => <div key={conversation.id} className={`ag-conversation${page === "chat" && current === conversation.id ? " is-active" : ""}`}>
              <button type="button" className="ag-conversation-main" title={`${conversation.title}
${formatTime(conversation.updatedAt)}`} onClick={() => { setCurrent(conversation.id); setPage("chat"); }}>
                {state.running.includes(conversation.id) && <Loader2 size={13} className="ag-spin" />}
                <span className="ag-conversation-title">{conversation.title}</span>
              </button>
              <Menu label="更多" trigger={<button type="button" className="ag-icon" aria-label="更多操作"><MoreHorizontal size={15} /></button>} items={[
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
                {current && loaded !== current ? <div className="min-h-0 flex-1" /> : <Conversation key={current ?? "new"} className="min-h-0">
                  <ConversationContent className="mx-auto min-h-full w-full max-w-3xl gap-4 px-6 pt-8 pb-10">
                    {!current && <div className="flex flex-1 flex-col items-center justify-center gap-4 pb-10 text-center">
                      <div className="flex size-12 items-center justify-center rounded-2xl bg-secondary text-foreground"><Bot size={24} /></div>
                      <h1 className="font-semibold text-2xl tracking-tight">想怎么整理图片？</h1>
                      <p className="max-w-md text-muted-foreground text-sm leading-relaxed">用大白话告诉我就行。改数据之前我会先预演给你看，有不确定的地方会先问你；每次修改都能撤销，原图文件不会被改动。</p>
                      <Suggestions className="mt-3 max-w-2xl">{SUGGESTIONS.map(text => <Suggestion key={text} suggestion={text} onClick={value => { setInput(value); inputRef.current?.focus(); }} />)}</Suggestions>
                    </div>}
                    {items.map(renderItem)}
                    {running && <Message from="assistant" className="max-w-full">
                      {live.reasoning && <Reasoning isStreaming={!live.text} className="select-text"><ReasoningTrigger /><ReasoningContent className="ag-prose">{live.reasoning}</ReasoningContent></Reasoning>}
                      {live.text ? <MessageContent className="ag-prose w-full select-text"><MessageResponse isAnimating>{live.text}</MessageResponse></MessageContent>
                        : !live.reasoning && !toolRunning && <Shimmer className="text-sm" duration={1.2}>思考中…</Shimmer>}
                    </Message>}
                  </ConversationContent>
                  <ConversationScrollButton />
                </Conversation>}
                <div className="mx-auto w-full max-w-3xl flex-none px-6 pb-4">
                  <PromptInput onSubmit={() => void send()}>
                    <PromptInputTextarea ref={inputRef} value={input} autoFocus placeholder={running ? "助手正在处理…" : "描述你想怎么整理图片"} onChange={event => setInput(event.target.value)} />
                    <PromptInputFooter>
                      <PromptInputTools>
                        <PromptInputButton title="选择助手使用的模型" onClick={() => setSettingsOpen(true)}>
                          <Settings2 /><span className="truncate">{modelName ? `${modelName} · ${state?.preferences.effort || "medium"}` : "选择模型"}</span>
                        </PromptInputButton>
                      </PromptInputTools>
                      <PromptInputSubmit status={running ? "streaming" : sending ? "submitted" : "ready"} disabled={!input.trim() || sending} onStop={() => { if (current) void agentCancel(current); }} />
                    </PromptInputFooter>
                  </PromptInput>
                  <p className="mt-2 text-center text-muted-foreground text-xs">Enter 发送，Shift+Enter 换行 · 每次修改都能在「操作记录」里撤销</p>
                </div>
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
