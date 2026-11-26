import { useEffect, useRef } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** Secondary windows open hidden (`build_hidden_until_ready`); shows this one the first time `ready` is true. */
export function useRevealWindow(ready: boolean): void {
  const revealed = useRef(false);
  useEffect(() => {
    if (revealed.current || !ready) return;
    revealed.current = true;
    const appWindow = getCurrentWindow();
    void appWindow.show().then(() => appWindow.setFocus()).catch(() => {});
  }, [ready]);
}
