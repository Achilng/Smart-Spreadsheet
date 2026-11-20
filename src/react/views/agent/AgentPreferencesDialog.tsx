import { useCallback, useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { Check, ChevronDown } from "lucide-react";
import {
  agentModels, DEFAULT_CONTEXT_WINDOW, saveAgentPreferences, selectedAgentModel,
  type AgentChannel, type AgentPreferences, type AgentState,
} from "../../../lib/api/agent";
import { errorText } from "../../../lib/utils/format";
import { Button, Modal, Select } from "../../ui/controls";

const EFFORTS = [["minimal", "最快（minimal）"], ["low", "低（low）"], ["medium", "中（medium，推荐）"], ["high", "高（high，更慢）"], ["default", "模型默认"]] as const;
const RECOMMENDED = new Set(["gpt-6-luna【神秘】", "gpt-6-luna"].map(normalizeSearch));
const NO_MODELS: string[] = [];

type Catalog = { models?: string[]; loading: boolean; error?: string };
const catalogKey = (channel: AgentChannel) => JSON.stringify([channel.id, channel.catalogRevision]);

export function AgentPreferencesDialog({ state, open, onClose, onSaved }: {
  state: AgentState; open: boolean; onClose: () => void; onSaved: (preferences: AgentPreferences) => void;
}) {
  const [catalogs, setCatalogs] = useState<Record<string, Catalog>>({});
  const pending = useRef(new Set<string>());
  const loadModels = useCallback(async (channel: AgentChannel) => {
    const key = catalogKey(channel);
    if (pending.current.has(key)) return;
    pending.current.add(key);
    setCatalogs(current => ({ ...current, [key]: { models: current[key]?.models, loading: true } }));
    try {
      const models = (await agentModels(state.libraryId, channel)).filter(model => model.trim());
      setCatalogs(current => ({ ...current, [key]: { models, loading: false } }));
    } catch (error) {
      setCatalogs(current => ({ ...current, [key]: { models: current[key]?.models, loading: false, error: errorText(error) } }));
    } finally {
      pending.current.delete(key);
    }
  }, [state.libraryId]);

  return open && <PreferencesForm state={state} catalogs={catalogs} loadModels={loadModels} onClose={onClose} onSaved={onSaved} />;
}

function PreferencesForm({ state, catalogs, loadModels, onClose, onSaved }: {
  state: AgentState; catalogs: Record<string, Catalog>; loadModels: (channel: AgentChannel) => Promise<void>;
  onClose: () => void; onSaved: (preferences: AgentPreferences) => void;
}) {
  const saved = state.preferences;
  const [channelId, setChannelId] = useState(saved.channelId);
  const channel = state.channels.find(item => item.id === channelId) ?? state.channels[0];
  const [modelDrafts, setModelDrafts] = useState<Record<string, string>>({});
  const model = channel ? modelDrafts[channel.id] ?? selectedAgentModel(saved, channel.id) : "";
  const [effort, setEffort] = useState(saved.effort || "medium");
  const [contextK, setContextK] = useState(String(Math.round((saved.contextWindow || DEFAULT_CONTEXT_WINDOW) / 1000)));
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [expanded, setExpanded] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const catalog = channel ? catalogs[catalogKey(channel)] : undefined;

  useEffect(() => {
    if (channel?.hasKey && !catalog) void loadModels(channel);
  }, [channel, catalog, loadModels]);

  const changeChannel = (value: string) => {
    setChannelId(value);
    setExpanded(false);
    setSaveError("");
  };
  const chooseModel = (value: string) => {
    if (!channel) return;
    setModelDrafts(current => ({ ...current, [channel.id]: value }));
    setSaveError("");
    setExpanded(false);
    trigger.current?.focus();
  };

  const save = async () => {
    if (!channel || saving || !(model || channel.model.trim())) return;
    const k = Number(contextK);
    if (!Number.isInteger(k) || k < 16 || k > 10000) { setSaveError("上下文长度请填 16 到 10000 之间的整数（单位：k tokens）。"); return; }
    setSaveError("");
    setSaving(true);
    const channelModels = { ...saved.channelModels };
    // Older preferences stored only the active channel, so retain its last choice on the first save.
    if (saved.channelId && !channelModels[saved.channelId]) {
      channelModels[saved.channelId] = { model: saved.model, recent: saved.model ? [saved.model] : [] };
    }
    const recent = channelModels[channel.id]?.recent ?? [];
    channelModels[channel.id] = { model, recent: model ? [model, ...recent.filter(item => item !== model)].slice(0, 5) : recent };
    const preferences = { channelId: channel.id, model, effort, channelModels, contextWindow: k * 1000 };
    try {
      await saveAgentPreferences(state.libraryId, preferences);
      onSaved(preferences);
      onClose();
    } catch (error) {
      setSaveError(errorText(error));
      setSaving(false);
    }
  };

  return <Modal open onClose={onClose} busy={saving} title="助手使用的模型" width={530}
    description="渠道和 Key 来自工具箱「模型设置」。助手需要支持 Responses 接口和工具调用的模型。"
    onEscapeKeyDown={event => {
      if (!expanded || event.isComposing) return;
      event.preventDefault();
      setExpanded(false);
      trigger.current?.focus();
    }}
    footer={<>
      <Button onClick={onClose} disabled={saving}>取消</Button>
      <Button variant="primary" onClick={() => void save()} disabled={saving || !(model || channel?.model.trim())}>{saving ? "正在保存…" : "保存"}</Button>
    </>}>
    {!channel ? <p className="ag-hint">还没有模型渠道，请先到工具箱的「模型设置」里添加。</p> : <div className="ag-form">
      <div className="ag-model-settings">
        <div className="field"><span className="field-label">渠道</span>
          <Select label="渠道" value={channel.id} onChange={changeChannel} disabled={saving}
            options={state.channels.map(item => [item.id, item.hasKey ? item.name : `${item.name}（没有 Key）`] as const)} />
        </div>
        <div className="field"><span className="field-label">思考强度</span>
          <Select label="思考强度" value={effort} onChange={setEffort} options={EFFORTS} disabled={saving} />
        </div>
      </div>
      <div className="field">
        <div className="ag-model-heading"><span className="field-label">模型</span>
          <Button size="sm" variant="ghost" disabled={!channel.hasKey || catalog?.loading || saving} onClick={() => void loadModels(channel)}>
            {catalog?.loading ? "正在获取…" : catalog?.error ? "重试" : "刷新列表"}
          </Button>
        </div>
        <button ref={trigger} type="button" className="r-input ag-model-trigger" data-autofocus
          aria-expanded={expanded} aria-label={`选择模型，${model || (channel.model ? `跟随渠道默认 · ${channel.model}` : "尚未选择")}`}
          disabled={saving} onClick={() => setExpanded(value => !value)}>
          <span>{model || (channel.model ? `跟随渠道默认 · ${channel.model}` : "选择或输入模型")}</span><ChevronDown size={14} aria-hidden="true" />
        </button>
        <div className="ag-hint ag-model-status" role="status">
          {!channel.hasKey ? "请在工具箱为此渠道填写 Key。也可以先手动填写模型名称。"
            : catalog?.error ? `${catalog.error}${catalog.models?.length ? " 仍可使用上次获取的列表。" : " 也可以手动填写模型名称。"}`
            : catalog?.models?.length ? `渠道返回 ${catalog.models.length} 个模型`
            : catalog?.loading ? "正在读取渠道的模型列表…"
            : catalog?.models ? "渠道返回了空列表，可以手动填写模型名称。" : ""}
        </div>
        {expanded && <ModelPicker channel={channel} model={model} models={catalog?.models ?? NO_MODELS}
          recent={saved.channelModels[channel.id]?.recent ?? NO_MODELS} onChoose={chooseModel} />}
      </div>
      {!expanded && <p className="ag-hint">如果模型不支持调节思考强度，请选「模型默认」。</p>}
      {!expanded && <div className="field"><span className="field-label">上下文长度</span>
        <div className="ag-context-input">
          <input type="number" className="r-input" inputMode="numeric" min={16} max={10000} step={1} value={contextK} disabled={saving}
            aria-label="上下文长度（千 tokens）" onChange={event => { setContextK(event.target.value); setSaveError(""); }} />
          <span>k tokens</span>
        </div>
        <p className="ag-hint">对话快到这个长度时，才会压缩较早的查询结果。不清楚模型上限就保持 200k。</p>
      </div>}
      {saveError && <p className="ag-hint" role="alert">{saveError}</p>}
    </div>}
  </Modal>;
}

function normalizeSearch(value: string) {
  return value.normalize("NFKC").toLowerCase();
}

function ModelPicker({ channel, model, models, recent, onChoose }: {
  channel: AgentChannel; model: string; models: string[]; recent: string[]; onChoose: (model: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [activeModel, setActiveModel] = useState<string | null>(null);
  const search = useRef<HTMLInputElement>(null);
  const listId = useId();
  const typed = query.trim();
  const ordered = useMemo(() => [...new Set([
    ...(model ? [model] : []),
    ...recent,
    ...models.filter(name => RECOMMENDED.has(normalizeSearch(name))),
    ...models,
  ])], [model, models, recent]);
  const terms = normalizeSearch(typed).split(/\s+/);
  const matches = ordered.filter(name => {
    const normalized = normalizeSearch(name);
    return terms.every(term => normalized.includes(term));
  });
  const manual = typed && !ordered.includes(typed) ? typed : null;
  const options = typed
    ? [...matches.slice(0, 3), ...(manual ? [manual] : []), ...matches.slice(3)]
    : [...new Set([model, "", ...ordered])].filter(name => name || channel.model.trim());
  const active = activeModel === null ? -1 : options.indexOf(activeModel);
  const optionId = (index: number) => `${listId}-${index}`;

  useEffect(() => { search.current?.focus(); }, []);

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing || event.keyCode === 229 || !options.length) return;
    if (event.key === "Enter") {
      const exactMatch = ordered.includes(typed) ? typed : null;
      const selected = options[active] ?? (typed ? exactMatch ?? matches[0] ?? manual : null);
      if (selected == null) return;
      event.preventDefault();
      onChoose(selected);
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const next = event.key === "ArrowDown" ? Math.min(active + 1, options.length - 1)
        : active < 0 ? options.length - 1 : Math.max(active - 1, 0);
      setActiveModel(options[next]);
      document.getElementById(optionId(next))?.scrollIntoView({ block: "nearest" });
    }
  };

  return <div className="ag-model-picker">
    <input ref={search} type="text" className="r-input" role="combobox" autoComplete="off" spellCheck={false}
      aria-label="搜索或输入模型名称" aria-controls={listId} aria-expanded="true" aria-autocomplete="list"
      aria-activedescendant={active >= 0 ? optionId(active) : undefined}
      value={query} onChange={event => { setQuery(event.target.value); setActiveModel(null); }} onKeyDown={onKeyDown}
      placeholder="搜索或粘贴模型名称" />
    <div className="ag-model-list" id={listId} role="listbox" aria-label="模型选项">
      {options.map((name, index) => {
        const label = name === manual ? "手动使用" : name === model ? "当前选择"
          : recent.includes(name) ? "最近使用" : RECOMMENDED.has(normalizeSearch(name)) ? "推荐" : "";
        return <div key={name} id={optionId(index)} role="option" aria-selected={name === model}
          className={index === active ? "ag-model-option is-active" : "ag-model-option"}
          onMouseDown={event => event.preventDefault()} onClick={() => onChoose(name)}>
          <span>{name || `跟随渠道默认 · ${channel.model}`}</span>
          {label && <small>{label}</small>}
          {name === model && <Check size={14} aria-hidden="true" />}
        </div>;
      })}
      {!options.length && <div className="ag-hint ag-model-empty">可以在上面输入完整模型名称。</div>}
    </div>
  </div>;
}
