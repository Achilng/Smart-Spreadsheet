import { mount } from "svelte";

import App from "./App.svelte";
import "./app.css";
import CompareWindow from "./lib/views/compare/CompareWindow.svelte";
import ToolboxWindow from "./lib/views/tools/ToolboxWindow.svelte";

const target = document.querySelector<HTMLDivElement>("#app");

if (!target) {
  throw new Error("Missing #app root element");
}

const windowKind = new URLSearchParams(window.location.search).get("window");
const Root = windowKind === "toolbox"
  ? ToolboxWindow
  : windowKind === "compare"
    ? CompareWindow
    : App;

// 开发期 IPC 模拟（?mock=1）：先装好 mock 再挂载，生产构建摇树剔除。
if (import.meta.env.DEV && new URLSearchParams(window.location.search).has("mock")) {
  void import("./lib/dev/ipc-mock").then(module => {
    module.installIpcMock();
    mount(Root, { target });
  });
} else {
  mount(Root, { target });
}
