import { create } from "zustand";
import type { ViewMode } from "../../lib/utils/view-modes";

interface WorkspaceState {
  viewMode: ViewMode;
  detailOpen: boolean;
  galleryCardSize: number;
  tableRowHeight: number;
  setView: (viewMode: ViewMode) => void;
  setDetailOpen: (detailOpen: boolean) => void;
  setSize: (value: number) => void;
}

export const useWorkspace = create<WorkspaceState>((set, get) => ({
  viewMode: "gallery", detailOpen: true, galleryCardSize: 190, tableRowHeight: 64,
  setView: viewMode => set({ viewMode }),
  setDetailOpen: detailOpen => set({ detailOpen }),
  setSize: value => set(get().viewMode === "table" ? { tableRowHeight: value } : { galleryCardSize: value }),
}));
