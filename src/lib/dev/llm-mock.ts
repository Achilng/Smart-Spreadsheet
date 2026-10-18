/** Development-only IPC fixture for reviewing the embedded workbench without model calls. */
import type { LlmChannel, LlmItem, LlmJob, LlmSettings, LlmSnapshot } from "../api/local-llm";
// Fixture values only; production defaults always come from llm_snapshot.
const mockIgnoredTags = ["mock quality", "mock resolution"];

export function createLlmMock() {
  let settings: LlmSettings = { autoImport: false, autoResume: true, batchSize: 10, reasoning: "medium", channels: [], ignoredTags: [...mockIgnoredTags] };
  const jobs: LlmJob[] = [];
  const results = new Map<string, LlmItem[]>();
  return (command: string, payload: Record<string, unknown>): unknown => {
    switch (command) {
      case "llm_snapshot": return {
        libraryId: "mock-library", defaultIgnoredTags: [...mockIgnoredTags], settings: structuredClone(settings), jobs: structuredClone(jobs),
        progress: { libraryId: "mock-library", jobId: jobs.find(j => j.state === "running")?.id ?? "", total: 8, done: 3, failed: 0, inFlight: 1, queued: 0, message: "", running: jobs.some(j => j.state === "running") },
        live: jobs.some(j => j.state === "running") ? [{ id: 1, channel: settings.channels[0]?.name ?? "示例渠道", model: settings.channels[0]?.model ?? "example-model", count: 5, text: '{"items":[{"id":"1","status":"ok","artist_string":"artist:alpha, watercolor"' }] : [],
        liveTotal: jobs.some(j => j.state === "running") ? 1 : 0,
      } satisfies LlmSnapshot;
      case "llm_save_settings": {
        const next = structuredClone(payload.settings) as LlmSettings;
        next.ignoredTags = [...new Set(next.ignoredTags.map(tag => tag.replace(/[_\s]+/g, " ").trim().toLowerCase()).filter(Boolean))];
        next.channels = next.channels.map(c => {
          const old = settings.channels.find(v => v.id === c.id && v.baseUrl === c.baseUrl);
          const { key, ...rest } = c;
          return { ...rest, hasKey: key === undefined ? old?.hasKey ?? false : !!key.trim() };
        }); settings = next; return structuredClone(settings);
      }
      case "llm_estimate": return { rows: 12, skipped: 2, uniquePrompts: 8, affectedRows: payload.allMatching ? 18 : 10, reusablePrompts: payload.includeProcessed ? 0 : 5, waitingPrompts: payload.includeProcessed ? 0 : 1, requestPrompts: payload.includeProcessed ? 8 : 2 };
      case "refresh_artist_matching": return { rows: 12, changedRows: 4, groupsBefore: 6, groupsAfter: 3, mergedRepresentatives: 1 };
      case "llm_models": {
        const channel = payload.channel as LlmChannel;
        if (!channel.key && !channel.hasKey) throw Error("请先填写 API Key");
        return ["example-model", "example-fast", "example-pro"];
      }
      case "llm_create": {
        const id = String(Date.now());
        jobs.unshift({ id, createdAt: new Date().toISOString(), state: "running", promptVersion: "style-extraction-v2", batchSize: settings.batchSize, reasoning: settings.reasoning, channelIds: payload.channelIds as string[], message: "", total: 8, done: 3, failed: 0, empty: 1, applied: 4, reuse: true });
        results.set(id, Array.from({ length: 8 }, (_, i) => ({ hash: `mock-${i}`, prompt: i === 2 ? "1girl, silver hair, beach" : "artist:alpha, watercolor, masterpiece, 1girl, blue eyes", status: i < 2 ? "ok" : i === 2 ? "none" : "pending", artists: i < 2 ? "artist:alpha, watercolor, masterpiece, " : "", model: i < 3 ? settings.channels[0]?.model ?? "example-model" : "", channel: i < 3 ? settings.channels[0]?.name ?? "示例渠道" : "", error: "", attempts: 0, rowId: i + 1 })));
        return id;
      }
      case "llm_items": return structuredClone((results.get(String(payload.jobId)) ?? []).slice(Number(payload.page) * 20, (Number(payload.page) + 1) * 20));
      case "llm_action": {
        const job = jobs.find(j => j.id === payload.jobId); if (!job) throw Error("任务不存在");
        const action = String(payload.action);
        if (["resume", "retry", "channels"].includes(action)) job.channelIds = payload.channelIds as string[];
        if (action !== "channels") job.state = ({ pause: "paused", resume: "running", retry: "running", undo: "undone", cancel: "cancelled" })[action] ?? job.state;
        job.message = action === "undo" ? "已撤销；0 张后续发生变化，保留现状" : ""; return;
      }
      case "llm_export": return;
      default: throw Error(`未实现的模拟命令：${command}`);
    }
  };
}
