import { create } from "zustand";
import { errorText } from "../../lib/utils/format";
import { notify } from "./notices";

export type DetailPromptMode = "positive" | "artists";
const storageKey = "smart-spreadsheet.detail-prompt-mode";
const parseMode = (value: string | null): DetailPromptMode => value === "artists" ? "artists" : "positive";

function savedMode(): DetailPromptMode {
  try { return parseMode(localStorage.getItem(storageKey)); }
  catch { return "positive"; }
}

export const useDetailPreferences = create<{ promptMode: DetailPromptMode }>(() => ({ promptMode: savedMode() }));

export function setDetailPromptMode(promptMode: DetailPromptMode): void {
  useDetailPreferences.setState({ promptMode });
  try { localStorage.setItem(storageKey, promptMode); }
  catch (error) { notify(`已切换显示，但无法记住设置：${errorText(error)}`, "error"); }
}

// Also keep already-open windows in sync; no library data is changed.
function syncPreference(event: StorageEvent): void {
  if (event.storageArea === localStorage && (event.key === storageKey || event.key === null)) {
    useDetailPreferences.setState({ promptMode: parseMode(event.newValue) });
  }
}
window.addEventListener("storage", syncPreference);
if (import.meta.hot) import.meta.hot.dispose(() => window.removeEventListener("storage", syncPreference));
