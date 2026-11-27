import { useEffect, useState } from "react";
import { check } from "@tauri-apps/plugin-updater";
import { CircleArrowUp, X } from "lucide-react";
import { openToolboxWindow } from "../../lib/windows/toolbox";
import { errorText } from "../../lib/utils/format";
import { notify } from "../state/notices";
import { Button } from "../ui/controls";

/** Leaves the first seconds after launch to loading the library. */
const STARTUP_DELAY_MS = 4000;
let pending: Promise<string | null> | null = null;

/** One silent check per app launch; any failure just means no reminder. */
function newerVersion(): Promise<string | null> {
  pending ??= new Promise(resolve => setTimeout(resolve, STARTUP_DELAY_MS))
    .then(() => check({ timeout: 15_000 }))
    .then(async update => {
      if (!update) return null;
      const version = update.version;
      await update.close().catch(() => {});
      return version;
    })
    .catch(() => null);
  return pending;
}

export function UpdateReminder() {
  const [version, setVersion] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);
  useEffect(() => {
    let cancelled = false;
    void newerVersion().then(value => { if (!cancelled && value) setVersion(value); });
    return () => { cancelled = true; };
  }, []);

  async function openUpdate() {
    setOpening(true);
    try {
      await openToolboxWindow("update");
      setVersion(null);
    } catch (error) {
      notify(`无法打开应用更新：${errorText(error)}`, "error");
    } finally { setOpening(false); }
  }

  if (!version) return null;
  return <div className="r-update-reminder" role="status">
    <button type="button" disabled={opening} onClick={() => void openUpdate()}>
      <CircleArrowUp size={18} />
      <span><strong>发现新版本 {version}</strong><small>{opening ? "正在打开…" : "点击前往「工具箱 → 应用更新」"}</small></span>
    </button>
    <Button variant="ghost" size="icon" aria-label="关闭更新提示" onClick={() => setVersion(null)}><X size={14} /></Button>
  </div>;
}
