import { create } from "zustand";
import { errorText } from "../../lib/utils/format";
import { notify } from "./notices";

export const useTasks = create<{ busy: boolean; label: string; progress: { processed: number; total: number; stage?: string } | null }>(() => ({ busy: false, label: "", progress: null }));

export async function runTask<T>(label: string, action: () => Promise<T>): Promise<T> {
  if (useTasks.getState().busy) throw new Error("当前有任务进行中，请稍后重试。");
  useTasks.setState({ busy: true, label });
  try { return await action(); }
  finally { useTasks.setState({ busy: false, label: "", progress: null }); }
}

export async function runAction(action: () => Promise<void>, label = "处理任务"): Promise<void> {
  try { await runTask(label, action); }
  catch (error) { notify(errorText(error), "error"); }
}
