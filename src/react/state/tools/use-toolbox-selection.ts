import { useEffect } from "react";
import { create } from "zustand";
import { emitTo, listen } from "@tauri-apps/api/event";
import type { RowSelection } from "../../../lib/api";
import type { ToolboxSelectionSnapshot } from "../../../lib/windows/toolbox";
import { errorText } from "../../../lib/utils/format";

const useSnapshot = create<{
  selection: ToolboxSelectionSnapshot | null; ready: boolean; error: string | null;
}>(() => ({ selection: null, ready: false, error: null }));
let consumers = 0, generation = 0;
let connection: Promise<void> | null = null;
let stop: (() => void) | undefined;

function acquire(): () => void {
  if (++consumers === 1) {
    const request = ++generation;
    connection = listen<ToolboxSelectionSnapshot>("main://selection-changed", event => {
      if (request === generation) useSnapshot.setState({ selection: event.payload, ready: true, error: null });
    }).then(unlisten => {
      if (request !== generation) unlisten(); else stop = unlisten;
    }).catch(error => {
      if (request === generation) useSnapshot.setState({ selection: null, ready: false, error: `无法监听主窗口选区：${errorText(error)}` });
    });
  }
  return () => {
    if (--consumers) return;
    generation++; stop?.(); stop = undefined; connection = null;
    useSnapshot.setState({ selection: null, ready: false, error: null });
  };
}

async function refresh(): Promise<void> {
  const request = generation;
  await connection;
  if (request !== generation || !stop) return;
  try { await emitTo("main", "toolbox://request-selection"); }
  catch (error) {
    if (request === generation) useSnapshot.setState({ selection: null, ready: false, error: `无法读取主窗口选区：${errorText(error)}` });
  }
}

/** All mounted tools share one listener; snapshot requests wait for registration. */
export function useToolboxSelection(active: boolean) {
  const state = useSnapshot();
  useEffect(acquire, []);
  useEffect(() => { if (active) void refresh(); }, [active]);
  return { ...state, refresh };
}

export function toolboxScopeSelection(scope: string, snapshot: ToolboxSelectionSnapshot | null): RowSelection | null {
  if (scope === "all") return null;
  const selection = scope === "selected" ? snapshot?.selection : scope === "filtered" ? snapshot?.filteredSelection : undefined;
  if (!selection) throw new Error("尚未获取主窗口范围，请返回主窗口后重试。");
  return selection;
}
