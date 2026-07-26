import type { ViewMode } from "../../stores/app-state.svelte";

/** 主窗口五个视图的顺序与文案（分段控件 / 画布大标题共用） */
export const VIEW_MODES: { mode: ViewMode; label: string }[] = [
  { mode: "gallery", label: "画廊" },
  { mode: "table", label: "表格" },
  { mode: "group", label: "分组" },
  { mode: "duplicates", label: "重复" },
  { mode: "promptDocs", label: "提示词" },
];

export function viewLabel(mode: ViewMode): string {
  return VIEW_MODES.find(view => view.mode === mode)?.label ?? "";
}
