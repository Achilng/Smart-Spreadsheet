import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";

// Initialize IPC before importing window-aware modules. No mock reaches a production build.
async function start(): Promise<void> {
  if (import.meta.env.DEV && new URLSearchParams(location.search).has("mock")) {
    const { installIpcMock } = await import("../lib/dev/ipc-mock");
    installIpcMock();
  }
  const { App } = await import("./App");
  const root = document.getElementById("app");
  if (!root) throw new Error("Missing #app root element");
  createRoot(root).render(<StrictMode><App /></StrictMode>);
}

void start();
