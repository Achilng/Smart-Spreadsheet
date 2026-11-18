import { create } from "zustand";
import type { RowRecord } from "../../lib/api";
import { useLibrary, useRows } from "./library";
import { useWorkspace } from "./workspace";

type GalleryPick = (
  | { kind: "material"; label: string }
  | { kind: "compare"; sampleId: number }
) & { onFinish: (row: RowRecord | null) => void };

export const useGalleryPick = create<{ pick: GalleryPick | null }>(() => ({ pick: null }));

export function startGalleryPick(pick: GalleryPick) {
  useRows.setState({ activeRow: null });
  useGalleryPick.setState({ pick });
  useWorkspace.getState().setView("gallery");
}

export function finishGalleryPick(row: RowRecord | null = null) {
  const { pick } = useGalleryPick.getState();
  if (!pick || (pick.kind === "compare" && pick.sampleId === row?.id)) return;
  useGalleryPick.setState({ pick: null });
  pick.onFinish(row);
}

useWorkspace.subscribe((state, previous) => {
  if (state.viewMode !== previous.viewMode && state.viewMode !== "gallery") useGalleryPick.setState({ pick: null });
});
useLibrary.subscribe((state, previous) => {
  if (state.snapshot?.dataDirectory !== previous.snapshot?.dataDirectory) useGalleryPick.setState({ pick: null });
});
