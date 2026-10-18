import { useEffect, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { Plus, Settings2, Play, Pause, RotateCcw, CheckCircle2 } from "lucide-react";
import { llmSnapshot, saveLlmSettings, estimateLlm, createLlm, actLlm, llmItems, llmModels, exportLlm, type LlmSnapshot, type LlmSettings, type LlmChannel, type LlmItem, type LlmEstimate, type LlmJob } from "../../../lib/api/local-llm";
import { useToolboxSelection, toolboxScopeSelection } from "../../state/tools/use-toolbox-selection";
import { errorText } from "../../../lib/utils/format";
import { notify } from "../../state/notices";
import { Button, Checkbox } from "../../ui/controls";
import { ToolPage, ToolCard, ToolError, ToolMetrics } from "./shared";
import StyleExchangeTool from "./StyleExchangeTool";
import { refreshArtistMatching, type ArtistMatchingRefresh } from "../../../lib/api/local-llm";
import "./style-extraction.css";

const states: Record<string, string> = { queued: "等待提取", running: "正在提取", paused: "已暂停", blocked: "需要处理", completed: "已完成", cancelled: "已结束", undone: "已撤销" };
const itemStates: Record<string, string> = { pending: "等待提取或复用", ok: "已完成", none: "无画风", error: "失败" };

function PromptHighlight({ item }: { item: LlmItem }) {
  const index = item.artists ? item.prompt.indexOf(item.artists) : -1;
  return <pre className="llm-prompt">{index < 0 ? item.prompt : <>{item.prompt.slice(0, index)}<mark>{item.artists}</mark>{item.prompt.slice(index + item.artists.length)}</>}</pre>;
}

export default function StyleExtractionTool({ active }: { active: boolean }) {
  const [snapshot, setSnapshot] = useState<LlmSnapshot | null>(null);
  const [draft, setDraft] = useState<LlmSettings | null>(null);
  const [dirty, setDirty] = useState(false), [tab, setTab] = useState("tasks"), [error, setError] = useState<string | null>(null), [busy, setBusy] = useState(false);
  const { selection, error: selectionError } = useToolboxSelection(active);
  const [scope, setScope] = useState("all"), [include, setInclude] = useState(false), [allMatching, setAllMatching] = useState(false);
  const [estimate, setEstimate] = useState<LlmEstimate | null>(null), [channels, setChannels] = useState<string[]>([]);
  const [channelId, setChannelId] = useState(""), [models, setModels] = useState<string[]>([]);
  const [jobId, setJobId] = useState(""), [page, setPage] = useState(0), [items, setItems] = useState<LlmItem[]>([]);
  const [confirmUndo, setConfirmUndo] = useState(false), [livePage, setLivePage] = useState(0);
  const [matchingResult, setMatchingResult] = useState<ArtistMatchingRefresh | null>(null);
  const initialized = useRef(false), pending = useRef(false), alive = useRef(true);
  const activeJob = snapshot?.jobs.find(job => job.id === jobId);
  const channel = draft?.channels.find(value => value.id === channelId);
  const enabled = snapshot?.settings.channels.filter(value => value.enabled) ?? [];

  async function refresh() {
    const next = await llmSnapshot(livePage);
    if (!alive.current) return;
    setSnapshot(next);
    if (!initialized.current) {
      initialized.current = true; setDraft(next.settings);
      setChannels(next.settings.channels.filter(c => c.enabled).map(c => c.id));
      setChannelId(next.settings.channels[0]?.id ?? ""); setJobId(next.jobs[0]?.id ?? "");
    }
  }
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  useEffect(() => { setEstimate(null); }, [selection]);
  useEffect(() => {
    if (!active) return;
    let stopped = false, loading = false;
    const load = async () => { if (loading) return; loading = true; try { await refresh(); } catch (e) { if (!stopped) setError(errorText(e)); } finally { loading = false; } };
    void load();
    const timer = setInterval(() => void load(), 1500);
    return () => { stopped = true; clearInterval(timer); };
  }, [active, livePage]);
  useEffect(() => { if (snapshot && livePage * 40 >= snapshot.liveTotal && livePage) setLivePage(0); }, [snapshot?.liveTotal, livePage]);
  useEffect(() => { setPage(0); setConfirmUndo(false); setItems([]); }, [jobId]);
  useEffect(() => {
    if (!active || !snapshot || !jobId) return;
    let cancelled = false;
    void llmItems(snapshot.libraryId, jobId, page).then(result => { if (!cancelled) setItems(result); }).catch(e => { if (!cancelled) setError(errorText(e)); });
    return () => { cancelled = true; };
  }, [active, snapshot, jobId, page]);
  async function perform(fn: () => Promise<void>) {
    if (pending.current) return; pending.current = true; setBusy(true); setError(null);
    try { await fn(); } catch (e) { setError(errorText(e)); }
    finally { pending.current = false; if (alive.current) setBusy(false); }
  }
  function updateChannel(patch: Partial<LlmChannel>) {
    setDraft(current => current && ({ ...current, channels: current.channels.map(c => c.id === channelId ? { ...c, ...patch } : c) })); setDirty(true);
  }
  async function saveSettings() {
    if (!draft || !snapshot) return;
    const saved = await saveLlmSettings(snapshot.libraryId, draft);
    setDraft(saved); setDirty(false); setChannels(current => saved.channels.filter(c => c.enabled && (current.includes(c.id) || !snapshot.settings.channels.some(old => old.id === c.id))).map(c => c.id));
    setEstimate(null); setMatchingResult(null); await refresh(); notify("设置已保存，画风归类和后续提取均使用最新匹配规则。");
  }
  async function toggleSetting(key: "autoImport" | "autoResume", value: boolean) {
    if (!snapshot) return;
    const saved = await saveLlmSettings(snapshot.libraryId, { ...snapshot.settings, [key]: value });
    setDraft(current => current && ({ ...current, [key]: value })); setSnapshot(current => current && ({ ...current, settings: saved }));
  }
  const target = () => toolboxScopeSelection(scope, selection);
  async function create() {
    if (!snapshot) return;
    const result = await createLlm(snapshot.libraryId, target(), include, allMatching, channels);
    if (result) { setJobId(result); notify("任务已加入后台队列，优先复用结果，需要时才调用模型。"); }
    else notify("当前范围没有需要提取的正文，或已经在任务队列中。");
    setEstimate(null); await refresh();
  }
  async function action(job: LlmJob, name: string) {
    if (!snapshot) return;
    await actLlm(snapshot.libraryId, job.id, name, channels); setConfirmUndo(false); await refresh();
  }
  async function download() {
    if (!snapshot || !activeJob) return;
    const path = await save({ title: "导出提取结果", defaultPath: "画风提取结果.json", filters: [{ name: "JSON", extensions: ["json"] }] });
    if (path) { await exportLlm(snapshot.libraryId, activeJob.id, path); notify("结果已导出。"); }
  }
  const channelsPicker = <div className="llm-channel-picks">{enabled.map(c => <label key={c.id}><Checkbox checked={channels.includes(c.id)} disabled={busy} onCheckedChange={value => setChannels(current => value ? [...current, c.id] : current.filter(id => id !== c.id))} />{c.name}<small>{c.model} · {c.concurrency} 并发</small></label>)}</div>;

  return <ToolPage><div className="llm-tabs" role="tablist" aria-label="LLM 工作台"><Button role="tab" aria-selected={tab === "tasks"} variant={tab === "tasks" ? "primary" : "ghost"} onClick={() => setTab("tasks")}>提取任务</Button><Button role="tab" aria-selected={tab === "settings"} variant={tab === "settings" ? "primary" : "ghost"} onClick={() => setTab("settings")}><Settings2 size={15} />渠道设置{dirty ? " · 未保存" : ""}</Button><Button role="tab" aria-selected={tab === "exchange"} variant={tab === "exchange" ? "primary" : "ghost"} onClick={() => setTab("exchange")}>JSON 导入 / 导出</Button><Button role="tab" aria-selected={tab === "matching"} variant={tab === "matching" ? "primary" : "ghost"} onClick={() => setTab("matching")}>复用白名单{dirty ? " · 未保存" : ""}</Button></div>
    <ToolError error={error ?? selectionError} />
    {snapshot && <ToolCard><div className="llm-preferences"><label><Checkbox checked={snapshot.settings.autoImport} disabled={busy} onCheckedChange={value => void perform(() => toggleSetting("autoImport", value === true))} /><span><strong>导入图片时自动提取</strong><small>只处理本次新增图片；忽略白名单后相同的正文共用结果，不重复调用。</small></span></label><label><Checkbox checked={snapshot.settings.autoResume} disabled={busy} onCheckedChange={value => void perform(() => toggleSetting("autoResume", value === true))} /><span><strong>默认继续未完成工作</strong><small>重新打开资料库后继续未完成任务，手动暂停的任务保持暂停。</small></span></label></div><p>任务在本机后台运行，关闭工具箱继续处理，退出软件后保存进度。校验通过的结果自动写入画师串，无画风结果会清空旧值；失败保留原值。</p></ToolCard>}
    {!snapshot && !error && <ToolCard><p>正在读取 LLM 工作台…</p></ToolCard>}
    {tab === "settings" && draft && <ToolCard><div className="rt-row"><div><h3>模型渠道</h3><p>配置随当前资料库保存；Key 使用当前 Windows 账户加密，换电脑后需重新填写。</p></div><Button onClick={() => { const id = crypto.randomUUID(); setDraft({ ...draft, channels: [...draft.channels, { id, name: `渠道 ${draft.channels.length + 1}`, baseUrl: "", model: "", concurrency: 1, enabled: true, hasKey: false }] }); setChannelId(id); setModels([]); setDirty(true); }} disabled={busy}><Plus size={15} />新增渠道</Button></div>
      <div className="llm-channel-editor"><div className="llm-channel-list">{draft.channels.map(c => <button key={c.id} type="button" className={channelId === c.id ? "is-active" : ""} onClick={() => { setChannelId(c.id); setModels([]); }}><strong>{c.name || "未命名渠道"}</strong><small>{c.enabled ? "已启用" : "已停用"} · {c.model || "尚未填写模型"}</small></button>)}</div>
        {channel ? <div className="llm-channel-fields"><label>渠道名称<input className="r-input" value={channel.name} onChange={e => updateChannel({ name: e.target.value })} /></label><label>Base URL<input className="r-input" type="url" placeholder="https://api.example.com/v1" value={channel.baseUrl} onChange={e => { updateChannel({ baseUrl: e.target.value, key: "", hasKey: false }); setModels([]); }} /></label><label>API Key<input className="r-input" type="password" autoComplete="off" placeholder={channel.hasKey ? "已加密保存；无需再次填写" : "输入 API Key"} value={channel.key ?? ""} onChange={e => { updateChannel({ key: e.target.value }); setModels([]); }} /></label><div className="rt-actions"><Button variant="ghost" onClick={() => updateChannel({ key: "", hasKey: false })}>清除 Key</Button><small>修改地址后需重新填写 Key。</small></div><label>模型<input className="r-input" list="llm-models" value={channel.model} placeholder="填写完整模型名称" onChange={e => updateChannel({ model: e.target.value })} /><datalist id="llm-models">{models.map(model => <option key={model} value={model} />)}</datalist></label><Button disabled={busy} onClick={() => void perform(async () => { if (snapshot) { const values = await llmModels(snapshot.libraryId, channel); setModels(values); notify(`已获取 ${values.length} 个模型，可在模型输入框中搜索选择。`); } })}>获取模型列表</Button><label>并发请求数<input className="r-input" type="number" min={1} step={1} value={channel.concurrency} onChange={e => updateChannel({ concurrency: Number(e.target.value) })} /></label><label className="llm-inline"><Checkbox checked={channel.enabled} onCheckedChange={value => updateChannel({ enabled: value === true })} />启用此渠道</label><Button variant="ghost" onClick={() => { setDraft({ ...draft, channels: draft.channels.filter(c => c.id !== channel.id) }); setChannelId(draft.channels.find(c => c.id !== channel.id)?.id ?? ""); setDirty(true); }}>移除此渠道</Button></div> : <p>添加一个渠道，填写地址、Key 和模型即可开始。</p>}</div>
      <div className="rt-fields"><label>每次调用条数<input className="r-input" type="number" min={1} max={100} value={draft.batchSize} onChange={e => { setDraft({ ...draft, batchSize: Number(e.target.value) }); setDirty(true); }} /></label><label>推理等级<select value={draft.reasoning} onChange={e => { setDraft({ ...draft, reasoning: e.target.value }); setDirty(true); }}><option value="low">低</option><option value="medium">中</option><option value="high">高</option></select></label></div><p>多个渠道共享待处理队列；地址、模型和并发调整用于后续请求，已发出的请求继续完成。每次调用条数和推理等级用于新任务。</p><Button variant="primary" disabled={busy || !dirty} onClick={() => void perform(saveSettings)}>保存设置</Button>
    </ToolCard>}
    {tab === "tasks" && snapshot && <><ToolCard><h3>新建提取任务</h3><div className="rt-fields"><label>处理范围<select value={scope} onChange={e => { setScope(e.target.value); setEstimate(null); }}><option value="all">全部资料库</option><option value="selected">主窗口选中图片（{selection?.count ?? 0} 张）</option><option value="filtered">主窗口当前筛选结果</option></select></label><label><Checkbox checked={include} onCheckedChange={value => { setInclude(value === true); setEstimate(null); }} />强制重新提取（忽略历史结果）</label><label><Checkbox checked={allMatching} onCheckedChange={value => { setAllMatching(value === true); setEstimate(null); }} />同时更新全库匹配正文的图片</label></div>
      <h4>参与渠道</h4>{enabled.length ? channelsPicker : <p>还没有启用的渠道。<Button variant="ghost" onClick={() => setTab("settings")}>前往渠道设置</Button></p>}
      {estimate && <ToolMetrics items={[[estimate.rows, "范围内图片"], [estimate.skipped, "已处理或空正文"], [estimate.uniquePrompts, "去重后正文"], [estimate.affectedRows, "预计更新图片"], [estimate.reusablePrompts, "可直接复用"], [estimate.waitingPrompts, "等待已有任务"], [estimate.requestPrompts, "仍需提取正文"]]} />}
      <div className="rt-actions"><Button disabled={busy || (scope === "selected" && !selection?.count)} onClick={() => void perform(async () => setEstimate(await estimateLlm(snapshot.libraryId, target(), include, allMatching)))}>统计待处理内容</Button><Button variant="primary" disabled={busy || (scope === "selected" && !selection?.count)} onClick={() => void perform(create)}><Play size={14} />开始后台处理</Button></div><p>忽略白名单中的完整标签及多余分隔空白后，相同正文优先复用历史结果；正在处理的相同内容合并等待。成功的无画风结果同样复用，切换模型或渠道不会要求重新提取。只有仍需调用模型时才需要可用渠道。</p>{include && <p className="llm-warning">已开启强制重新提取：将再次调用模型，不使用历史结果；本任务内相同内容仍只提取一次。</p>}<p>默认只更新任务范围内的图片；任务记录按原始正文保留，复用条目会注明来源。提取期间被你改动的正文或画师串会跳过。</p></ToolCard>
      <ToolCard><h3>任务记录</h3>{!snapshot.jobs.length ? <p>还没有任务。手动创建任务，或开启导入图片时自动提取。</p> : <><div className="llm-job-list">{snapshot.jobs.map(job => <button type="button" key={job.id} className={job.id === jobId ? "is-active" : ""} onClick={() => setJobId(job.id)}><span><strong>{states[job.state] ?? job.state}</strong><small>{new Date(job.createdAt).toLocaleString()}</small></span><span>{job.done} / {job.total}<small>{job.failed ? `${job.failed} 条失败` : `${job.applied} 张已应用`}</small></span></button>)}</div>
        {activeJob && <section className="llm-job-detail"><div className="rt-row"><h3>{states[activeJob.state]} · {activeJob.done} / {activeJob.total}</h3><small>{activeJob.empty} 条无画风 · {activeJob.failed} 条失败</small></div><progress className="llm-progress" max={activeJob.total} value={activeJob.done + activeJob.failed} /><p>{activeJob.message || "成功结果会自动保存并应用，可以继续整理图片。"}</p><p>本任务渠道：{activeJob.channelIds.map(id => snapshot.settings.channels.find(c => c.id === id)?.name ?? "已移除渠道").join("、") || "未选择"}。继续 / 重试时采用上方当前勾选的渠道。</p><div className="rt-actions">
          {["queued", "running"].includes(activeJob.state) ? <Button disabled={busy} onClick={() => void perform(() => action(activeJob, "pause"))}><Pause size={14} />暂停</Button> : activeJob.state !== "undone" && activeJob.done + activeJob.failed < activeJob.total && <Button disabled={busy} onClick={() => void perform(() => action(activeJob, "resume"))}><Play size={14} />继续</Button>}
          {["queued", "running"].includes(activeJob.state) && <Button disabled={busy} onClick={() => void perform(() => action(activeJob, "channels"))}>应用渠道选择</Button>}
          {activeJob.failed > 0 && activeJob.state !== "undone" && <Button disabled={busy || !channels.length || activeJob.state === "running"} onClick={() => void perform(() => action(activeJob, "retry"))}><RotateCcw size={14} />重试失败项</Button>}
          {!['completed', 'cancelled', 'undone'].includes(activeJob.state) && <Button disabled={busy} onClick={() => void perform(() => action(activeJob, "cancel"))}>结束任务</Button>}
          <Button disabled={busy || !activeJob.done} onClick={() => void perform(download)}>导出结果 JSON</Button>
          {activeJob.applied > 0 && !["running", "queued", "undone"].includes(activeJob.state) && <Button disabled={busy} onClick={() => setConfirmUndo(true)}>撤销本任务写入</Button>}
        </div>{confirmUndo && <div className="llm-confirm"><p>撤销本任务写入的画师串与 LLM 来源；后续被修改的图片保持现状。撤销后如需重新提取，请新建任务。</p><Button disabled={busy} onClick={() => void perform(() => action(activeJob, "undo"))}>确认撤销</Button><Button variant="ghost" onClick={() => setConfirmUndo(false)}>返回</Button></div>}
          {snapshot.progress.jobId === activeJob.id && snapshot.liveTotal > 0 && <details><summary>实时请求 · {snapshot.liveTotal} 个进行中</summary><div className="llm-live">{snapshot.live.map(request => <section key={request.id}><strong>{request.channel} · {request.count} 条</strong><small>{request.model} · 以下为尚未校验的输出</small><pre>{request.text || "正在等待模型输出…"}</pre></section>)}</div>{snapshot.liveTotal > 40 && <div className="rt-actions"><Button disabled={!livePage} onClick={() => setLivePage(p => p - 1)}>上一页</Button><Button disabled={(livePage + 1) * 40 >= snapshot.liveTotal} onClick={() => setLivePage(p => p + 1)}>下一页</Button></div>}</details>}
          <h4>提取结果</h4>{items.map(item => <details className="llm-result" key={item.hash}><summary><span>{item.status === "ok" && <CheckCircle2 size={14} />}{itemStates[item.status]} · {item.artists || item.error || (item.status === "none" ? "未识别到画风" : item.prompt)}</span><small>{item.channel}</small></summary><PromptHighlight item={item} />{item.error && <p className="llm-warning">{item.error}</p>}<small>{item.model || "尚未调用"}</small></details>)}<div className="rt-actions"><Button disabled={!page} onClick={() => setPage(p => p - 1)}>上一页</Button><small>{page + 1} / {Math.max(1, Math.ceil(activeJob.total / 20))}</small><Button disabled={(page + 1) * 20 >= activeJob.total} onClick={() => setPage(p => p + 1)}>下一页</Button></div>
        </section>}</>}
      </ToolCard></>}
    {tab === "matching" && draft && <>
      <ToolCard><h3>画风与提取复用白名单</h3><p>每行一个完整标签。空格与下划线视为相同分隔，例如 very aesthetic 与 very_aesthetic 只需填写一个。比较时忽略白名单标签（包括加权写法）、多余分隔和末尾逗号；保留其余内容、画师权重和顺序。</p>
        <label className="llm-whitelist-label">忽略标签<textarea className="r-input llm-whitelist" rows={8} spellCheck={false} value={draft.ignoredTags.join("\n")} onChange={e => { setDraft({ ...draft, ignoredTags: e.target.value.split(/\r?\n/) }); setDirty(true); }} /></label>
        <p>默认仅忽略已约定的通用品质与文字限制标签；年份、newest、细节描述、rating:general、location 和 no signature 等仍参与比较。保存后自动更新已有画风归类，后续 LLM 复用也遵循同一套规则。</p>
        <div className="rt-actions"><Button variant="primary" disabled={busy || !dirty} onClick={() => void perform(saveSettings)}>保存设置</Button><Button disabled={busy || !snapshot} onClick={() => { setDraft({ ...draft, ignoredTags: [...(snapshot?.defaultIgnoredTags ?? [])] }); setDirty(true); }}>恢复默认白名单</Button></div>
      </ToolCard>
      <ToolCard><h3>刷新已有画风归类</h3><p>按已保存的规则重新整理整个资料库，让“只看当前画师串”“选中相同画师串”、按画师串去重、重复项及对比中的相同画师串得到一致结果。新导入和编辑会自动更新。</p>
        <p>不调用 LLM，不修改原文、历史提取结果或手动分组。合并后若有多个代表图，保留图片编号最小的一张；已有重复项名称优先沿用匹配后的名称，其余原名保留。仅剩被忽略标签的内容不作为同一画风归并。</p>
        <Button disabled={busy || dirty || !snapshot} onClick={() => void perform(async () => { if (!snapshot) return; const result = await refreshArtistMatching(snapshot.libraryId); setMatchingResult(result); setEstimate(null); notify(`已刷新 ${result.rows} 张图片的画风归类。`); })}><RotateCcw size={14} />{busy ? "正在处理…" : "刷新画风归类"}</Button>
        {dirty && <p>请先保存上方设置，再刷新。</p>}
        {matchingResult && <div role="status"><ToolMetrics items={[[matchingResult.rows, "已检查图片"], [matchingResult.changedRows, "归类已更新"], [matchingResult.groupsBefore, "刷新前画风数"], [matchingResult.groupsAfter, "刷新后画风数"]]} /><p>{matchingResult.mergedRepresentatives ? `合并了 ${matchingResult.mergedRepresentatives} 个重复或失效的代表图设置，图片均保留。` : "代表图设置无冲突。"}归类已同步到主窗口。</p></div>}
      </ToolCard>
    </>}
    {tab === "exchange" && <StyleExchangeTool active={active} />}
  </ToolPage>;
}
