import { mount } from "svelte";

import App from "./App.svelte";
import "./app.css";

const target = document.querySelector<HTMLDivElement>("#app");

if (!target) {
  throw new Error("Missing #app root element");
}

mount(App, { target });
