import { useEffect, useRef, useState } from "react";
import { createLlm, llmModels, llmSnapshot, saveLlmSettings, type LlmChannel } from "../../lib/api/local-llm";
import { errorText } from "../../lib/utils/format";
import { notify } from "../state/notices";
import { Button, Modal } from "../ui/controls";
import "./llm-setup.css";

const BASE_URL = "https://oneapi.hakoyu.com/v1";
const DEFAULT_MODEL = "gpt-6-luna【神秘】";
const CHANNEL_NAME = "大宇宙站";

export function YiyiLlmSetup({ libraryId, onBack, onComplete }: {
  libraryId: string;
  onBack: () => void;
  onComplete: () => void;
}) {
  const [step, setStep] = useState<"key" | "model" | "confirm">("key");
  const [key, setKey] = useState("");
  const [models, setModels] = useState<string[]>([]);
  const [model, setModel] = useState("");
  const [savedChannelId, setSavedChannelId] = useState("");
  const [busy, setBusy] = useState("");
  const [error, setError] = useState<string | null>(null);
  const pending = useRef(false), alive = useRef(true);
  const [channelId] = useState(() => crypto.randomUUID());
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);

  function channel(selectedModel: string, enteredKey: string): LlmChannel {
    return { id: channelId, name: CHANNEL_NAME, baseUrl: BASE_URL, model: selectedModel, concurrency: 1, enabled: true, hasKey: false, key: enteredKey };
  }

  async function perform(label: string, action: () => Promise<void>) {
    if (pending.current) return;
    pending.current = true; setBusy(label); setError(null);
    try { await action(); }
    catch (e) { if (alive.current) setError(errorText(e)); }
    finally { pending.current = false; if (alive.current) setBusy(""); }
  }

  async function saveConfiguration(selectedModel: string, enteredKey: string) {
    setBusy("正在保存…");
    // Read fresh settings so this shortcut preserves other channels and preferences.
    const snapshot = await llmSnapshot();
    if (!alive.current) return;
    if (snapshot.libraryId !== libraryId) throw new Error("资料库已切换，请关闭弹窗后重试。");
    const existing = snapshot.settings.channels.find(value => value.name === CHANNEL_NAME && value.baseUrl.replace(/\/+$/, "") === BASE_URL);
    const configured = { ...channel(selectedModel, enteredKey), id: existing?.id ?? channelId };
    const channels = existing
      ? snapshot.settings.channels.map(value => value.id === existing.id ? configured : value)
      : [...snapshot.settings.channels, configured];
    await saveLlmSettings(libraryId, { ...snapshot.settings, channels, batchSize: 10, reasoning: "high" });
    if (!alive.current) return;
    setSavedChannelId(configured.id); setKey(""); setModels([]); setStep("confirm");
  }

  async function submitKey() {
    if (!key.trim()) { setError("请填写大宇宙站的 Key。"); return; }
    await perform("正在验证…", async () => {
      const enteredKey = key.trim();
      const available = await llmModels(libraryId, channel(DEFAULT_MODEL, enteredKey));
      if (!alive.current) return;
      const choices = [...new Set(available.filter(value => value.trim()))];
      if (!choices.length) throw new Error("站点没有返回可用模型，请检查 Key 的模型权限，或稍后重试。");
      if (choices.includes(DEFAULT_MODEL)) await saveConfiguration(DEFAULT_MODEL, enteredKey);
      else { setModels(choices); setModel(""); setStep("model"); }
    });
  }

  async function startProcessing() {
    await perform("正在开始…", async () => {
      const jobId = await createLlm(libraryId, null, false, false, [savedChannelId]);
      if (!alive.current) return;
      notify(jobId ? "已开始处理未处理的图片，可以继续整理图片。" : "当前没有需要处理的图片。");
      onComplete();
    });
  }

  const errorMessage = error && <p className="r-llm-setup-error" role="alert">{error}</p>;
  if (step === "key") return <Modal key="key" open onClose={onBack} busy={!!busy} title="在此填写大宇宙站的key" width={460}
    footer={<Button variant="primary" disabled={!!busy || !key.trim()} onClick={() => void submitKey()}>{busy || "确认"}</Button>}>
    <form onSubmit={event => { event.preventDefault(); void submitKey(); }}>
      <input className="r-input" type="password" autoComplete="off" spellCheck={false} aria-label="大宇宙站 Key" placeholder="输入 Key" value={key} disabled={!!busy} onChange={event => { setKey(event.target.value); setError(null); }} />
      {errorMessage}
    </form>
  </Modal>;

  if (step === "model") return <Modal key="model" open onClose={onBack} busy={!!busy} title="请选择一个模型" description="没有找到默认模型，请从可用模型中选择。" width={460}
    footer={<><Button disabled={!!busy} onClick={() => { setStep("key"); setError(null); }}>重新填写 Key</Button><Button variant="primary" disabled={!!busy || !models.includes(model)} onClick={() => void perform("正在保存…", () => saveConfiguration(model, key.trim()))}>{busy || "确认"}</Button></>}>
    <select className="r-input" data-autofocus aria-label="可用模型" disabled={!!busy} value={model} onChange={event => { setModel(event.target.value); setError(null); }}>
      <option value="" disabled>选择模型</option>{models.map(value => <option key={value} value={value}>{value}</option>)}
    </select>
    {errorMessage}
  </Modal>;

  return <Modal key="confirm" open onClose={onComplete} busy={!!busy} title="自动处理未处理的图片？" width={460}
    footer={<><Button disabled={!!busy} onClick={onComplete}>取消</Button><Button variant="primary" data-autofocus disabled={!!busy} onClick={() => void startProcessing()}>{busy || "确认"}</Button></>}>
    <p>配置已保存。确认后会处理当前资料库中尚未处理的图片。</p>
    {errorMessage}
  </Modal>;
}
