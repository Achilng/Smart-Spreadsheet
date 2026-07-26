import { mount } from "svelte";
import { invoke } from "@tauri-apps/api/core";

import App from "./App.svelte";
import "./app.css";
import { initReveal } from "./lib/ui/reveal";
import ToolboxWindow from "./lib/views/tools/ToolboxWindow.svelte";

const target = document.querySelector<HTMLDivElement>("#app");

if (!target) {
  throw new Error("Missing #app root element");
}

// 玻璃视觉需确认系统透明效果可用；任何失败路径都留在不透明保底态。
invoke<boolean>("is_transparency_enabled")
  .then((enabled) => {
    if (enabled) {
      document.documentElement.dataset.glass = "on";
    }
  })
  .catch(() => {});

initReveal();

const windowKind = new URLSearchParams(window.location.search).get("window");
const Root = windowKind === "toolbox" ? ToolboxWindow : App;

mount(Root, { target });
