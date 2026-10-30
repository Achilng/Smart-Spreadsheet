import { useEffect, useRef, useState } from "react";
import { llmSnapshot } from "../../lib/api/local-llm";
import { errorText } from "../../lib/utils/format";
import { openToolboxWindow } from "../../lib/windows/toolbox";
import { useLibrary } from "../state/library";
import { notify } from "../state/notices";
import { Button, Modal } from "../ui/controls";
import { YiyiLlmSetup } from "./YiyiLlmSetup";

const DISMISSED_KEY = "smart-spreadsheet.llm-setup-reminder.dismissed";

/** Check once per main-window session, after a library is available. */
export function LlmSetupReminder() {
  const ready = useLibrary(state => state.loaded && !state.error && !state.snapshot?.startupError);
  const directory = useLibrary(state => state.snapshot?.dataDirectory);
  const checked = useRef(false);
  const [reminderDirectory, setReminderDirectory] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [libraryId, setLibraryId] = useState("");
  const [quickSetup, setQuickSetup] = useState(false);

  useEffect(() => {
    if (!ready || !directory || checked.current) return;
    let cancelled = false;
    async function check() {
      try {
        if (localStorage.getItem(DISMISSED_KEY) === "true") {
          checked.current = true;
          return;
        }
        const snapshot = await llmSnapshot();
        if (cancelled) return;
        checked.current = true;
        if (!snapshot.settings.channels.some(channel => channel.hasKey)) {
          setLibraryId(snapshot.libraryId); setReminderDirectory(directory!);
        }
      } catch (error) {
        if (!cancelled) {
          checked.current = true;
          notify(`无法检查 LLM Key 设置，可稍后在工具箱查看：${errorText(error)}`, "error");
        }
      }
    }
    void check();
    return () => { cancelled = true; };
  }, [ready, directory]);

  function dismissPermanently() {
    try {
      localStorage.setItem(DISMISSED_KEY, "true");
      setReminderDirectory(null);
    } catch (error) {
      notify(`无法保存“不再提醒”，请重试或选择“稍后再说”：${errorText(error)}`, "error");
    }
  }

  async function openSettings() {
    setBusy(true);
    try {
      await openToolboxWindow("llmSettings");
      setReminderDirectory(null);
    } catch (error) {
      notify(`无法打开渠道设置，请重试：${errorText(error)}`, "error");
    } finally { setBusy(false); }
  }

  const open = ready && !!reminderDirectory && reminderDirectory === directory;
  if (quickSetup && open) return <YiyiLlmSetup libraryId={libraryId} onBack={() => setQuickSetup(false)} onComplete={() => { setQuickSetup(false); setReminderDirectory(null); }} />;

  return <Modal open={open} onClose={() => setReminderDirectory(null)} busy={busy}
    title="填写 Key，开始 LLM 提取" description="当前资料库还没有保存 API Key。配置后，可以用 LLM 从图片提示词中提取画师串。" width={520}
    footer={<><Button variant="ghost" disabled={busy} onClick={dismissPermanently}>不再提醒</Button><Button disabled={busy} onClick={() => setReminderDirectory(null)}>稍后再说</Button><Button disabled={busy} onClick={() => void openSettings()}>{busy ? "正在打开…" : "去填写 Key"}</Button></>}>
    <Button className="r-llm-quick-setup" variant="primary" data-autofocus disabled={busy} onClick={() => setQuickSetup(true)}>我是衣衣，帮我一键配置！</Button>
    <p>前往「工具箱 → LLM提取 → 渠道设置」，添加渠道，填写 Base URL、API Key 和模型，然后点击「保存设置」。</p>
    <p>也可以稍后设置，继续整理图片。选择「不再提醒」后，本机以后启动时将不再显示此提示。</p>
  </Modal>;
}
