import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { llmSnapshot, type LlmProgress } from "../../lib/api/local-llm";

export const useLlmProgress = create<{ progress: LlmProgress | null }>(() => ({ progress: null }));
export async function listenLlmProgress(): Promise<() => void> {
  let active = true;
  const stop = await listen<LlmProgress>("llm://progress", event => { if (active) useLlmProgress.setState({ progress: event.payload }); });
  void llmSnapshot().then(snapshot => { if (active) useLlmProgress.setState({ progress: snapshot.progress }); }).catch(() => {});
  return () => { active = false; stop(); };
}
