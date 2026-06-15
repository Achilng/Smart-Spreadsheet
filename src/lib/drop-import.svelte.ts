import { getCurrentWebview } from "@tauri-apps/api/webview";

import { app } from "./app-state.svelte";
import { chooseRejectedImagesDirectory, runImageImport } from "./import-actions.svelte";

const ACCEPTED_EXTENSIONS = new Set([
  "zip", "7z", "rar",
  "png",
]);

export const dropState = $state({
  dragging: false,
  open: false,
  paths: [] as string[],
  busy: false,
});

function isAcceptedPath(p: string): boolean {
  const ext = p.split(".").pop()?.toLowerCase() ?? "";
  if (ACCEPTED_EXTENSIONS.has(ext)) return true;
  if (!p.includes(".")) return true;
  return false;
}

export function filterDroppedPaths(raw: string[]): string[] {
  return raw.filter(isAcceptedPath);
}

export function requestDropImport(paths: string[]): void {
  if (dropState.busy || app.busy) return;
  const valid = filterDroppedPaths(paths);
  if (valid.length === 0) return;
  dropState.paths = valid;
  dropState.open = true;
}

export function cancelDropImport(): void {
  if (!dropState.busy) {
    dropState.open = false;
    dropState.paths = [];
  }
}

export async function confirmDropImport(): Promise<void> {
  if (dropState.busy || dropState.paths.length === 0) return;

  const hasRejectedDir = Boolean(app.snapshot?.rejectedImagesDirectory);
  if (!hasRejectedDir) {
    const ok = await chooseRejectedImagesDirectory();
    if (!ok) return;
  }

  dropState.busy = true;
  try {
    for (const path of dropState.paths) {
      await runImageImport(path);
    }
  } finally {
    dropState.busy = false;
    dropState.open = false;
    dropState.paths = [];
  }
}

export function listenDragDrop(): () => void {
  let unlisten: (() => void) | null = null;

  getCurrentWebview()
    .onDragDropEvent((event) => {
      if (!app.snapshot?.dataDirectory) return;

      if (event.payload.type === "enter" || event.payload.type === "over") {
        dropState.dragging = true;
      } else if (event.payload.type === "leave") {
        dropState.dragging = false;
      } else if (event.payload.type === "drop") {
        dropState.dragging = false;
        if (!app.busy && !dropState.open) {
          requestDropImport(event.payload.paths);
        }
      }
    })
    .then((fn) => {
      unlisten = fn;
    });

  return () => {
    unlisten?.();
  };
}
