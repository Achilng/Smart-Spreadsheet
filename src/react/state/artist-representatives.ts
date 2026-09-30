import { create } from "zustand";
import { setArtistRepresentative, type RowRecord } from "../../lib/api";
import { errorText } from "../../lib/utils/format";
import { useLibrary } from "./library";
import { refreshLibrary } from "./library-changes";
import { notify } from "./notices";
import { runTask, useTasks } from "./tasks";
import { revealRow } from "./reveal-row";

interface Conflict { row: RowRecord; representativeId: number | null }
export const useArtistRepresentative = create<{ conflict: Conflict | null; error: string | null }>(() => ({ conflict: null, error: null }));

export function closeRepresentativeConflict(): void {
  if (!useTasks.getState().busy) useArtistRepresentative.setState({ conflict: null, error: null });
}

export async function markArtistRepresentative(row: RowRecord, enabled: boolean, expectedId: number | null = null): Promise<void> {
  if (useTasks.getState().busy) return;
  const directory = useLibrary.getState().snapshot?.dataDirectory;
  useArtistRepresentative.setState({ error: null });
  try {
    await runTask("设置画师串代表图", async () => {
      const result = await setArtistRepresentative(row.id, row.artists ?? "", enabled, expectedId);
      if (directory !== useLibrary.getState().snapshot?.dataDirectory) return;
      if (result.conflict) {
        useArtistRepresentative.setState({ conflict: { row, representativeId: result.representativeId } });
        return;
      }
      useArtistRepresentative.setState({ conflict: null });
      notify(enabled ? "已设为画师串代表图，按画师串去重时优先显示。" : "已取消画师串代表图。");
      try { await refreshLibrary({ resetScroll: false }); }
      catch (error) { notify(`标记已保存，但刷新失败：${errorText(error)}`, "error"); }
    });
  } catch (error) {
    const message = `设置代表图失败：${errorText(error)}`;
    useArtistRepresentative.setState({ error: message });
    notify(message, "error");
  }
}

export async function revealArtistRepresentative(): Promise<void> {
  const id = useArtistRepresentative.getState().conflict?.representativeId;
  if (id == null || useTasks.getState().busy) return;
  try {
    await runTask("定位代表图", () => revealRow(id));
    useArtistRepresentative.setState({ conflict: null, error: null });
    notify("已定位代表图，并清除图片筛选条件。");
  } catch (error) {
    useArtistRepresentative.setState({ error: `无法定位代表图：${errorText(error)}` });
  }
}

useLibrary.subscribe((state, previous) => {
  if (state.snapshot?.dataDirectory !== previous.snapshot?.dataDirectory) {
    useArtistRepresentative.setState({ conflict: null, error: null });
  }
});
