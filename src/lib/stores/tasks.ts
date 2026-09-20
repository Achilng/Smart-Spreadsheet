import { taskState } from "./task-state.svelte";
import { setNotice } from "./notices.svelte";
import { errorText } from "../utils/format";

export async function runAction(action: () => Promise<void>): Promise<void> {
  taskState.busy = true;
  setNotice(null);
  try {
    await action();
  } catch (error) {
    setNotice({ tone: "error", text: errorText(error) });
  } finally {
    taskState.busy = false;
  }
}
