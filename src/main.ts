import { mount } from "svelte";

import App from "./App.svelte";
import "./app.css";
import ToolboxWindow from "./lib/views/tools/ToolboxWindow.svelte";

const target = document.querySelector<HTMLDivElement>("#app");

if (!target) {
  throw new Error("Missing #app root element");
}

const windowKind = new URLSearchParams(window.location.search).get("window");
const Root = windowKind === "toolbox" ? ToolboxWindow : App;

mount(Root, { target });
