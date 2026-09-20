export interface SortPreview {
  key: string;
  beforeKey: string | null;
  lastKey: string | null;
  left: number;
  top: number;
  width: number;
}

interface SortOptions {
  disabled?: boolean;
  onpreview: (preview: SortPreview | null) => void;
  onmove: (key: string, index: number) => void;
}

/** Pointer capture avoids WebView's native file-drop interception. Order changes only on release. */
export function pointerSort(node: HTMLElement, initial: SortOptions) {
  let options = initial;
  let drag: { key: string; pointerId: number; startY: number; y: number; offset: number; left: number; width: number; moved: boolean } | null = null;
  let frame = 0;
  const view = node.ownerDocument.defaultView!;
  const rows = () => [...node.querySelectorAll<HTMLElement>("[data-sort-key]")];
  function destination() {
    const remaining = rows().filter(row => row.dataset.sortKey !== drag?.key);
    let index = remaining.findIndex(row => {
      const rect = row.getBoundingClientRect();
      return drag!.y < rect.top + rect.height / 2;
    });
    if (index < 0) index = remaining.length;
    return { index, beforeKey: remaining[index]?.dataset.sortKey ?? null, lastKey: remaining.at(-1)?.dataset.sortKey ?? null };
  }
  function preview() {
    if (!drag?.moved) return;
    options.onpreview({ key: drag.key, ...destination(), left: drag.left, top: drag.y - drag.offset, width: drag.width });
  }
  function scroll() {
    if (!drag) return;
    if (drag.moved) {
      const rect = node.getBoundingClientRect();
      const delta = drag.y < rect.top + 28 ? -Math.min(8, (rect.top + 28 - drag.y) / 4)
        : drag.y > rect.bottom - 28 ? Math.min(8, (drag.y - rect.bottom + 28) / 4) : 0;
      if (delta) { node.scrollTop += delta; preview(); }
    }
    frame = view.requestAnimationFrame(scroll);
  }
  function finish(commit = false) {
    if (!drag) return;
    const current = drag;
    const target = commit && current.moved ? destination().index : null;
    drag = null;
    view.cancelAnimationFrame(frame);
    if (node.hasPointerCapture(current.pointerId)) node.releasePointerCapture(current.pointerId);
    options.onpreview(null);
    if (target !== null) options.onmove(current.key, target);
  }
  function down(event: PointerEvent) {
    if (options.disabled || drag || event.button !== 0 || !event.isPrimary) return;
    const handle = (event.target as HTMLElement).closest<HTMLElement>("[data-sort-handle]");
    const row = handle?.closest<HTMLElement>("[data-sort-key]");
    if (!row || !node.contains(row) || handle?.matches(":disabled")) return;
    const rect = row.getBoundingClientRect();
    event.preventDefault();
    handle?.focus();
    node.setPointerCapture(event.pointerId);
    drag = { key: row.dataset.sortKey!, pointerId: event.pointerId, startY: event.clientY, y: event.clientY,
      offset: event.clientY - rect.top, left: rect.left, width: rect.width, moved: false };
    frame = view.requestAnimationFrame(scroll);
  }
  function move(event: PointerEvent) {
    if (!drag || event.pointerId !== drag.pointerId) return;
    event.preventDefault();
    drag.y = event.clientY;
    drag.moved ||= Math.abs(drag.y - drag.startY) >= 4;
    preview();
  }
  function up(event: PointerEvent) { if (event.pointerId === drag?.pointerId) { drag.y = event.clientY; finish(true); } }
  function cancel(event: PointerEvent) { if (event.pointerId === drag?.pointerId) finish(); }
  function key(event: KeyboardEvent) {
    if (event.key === "Escape" && drag) { event.preventDefault(); event.stopPropagation(); finish(); }
  }
  const blur = () => finish();
  node.addEventListener("pointerdown", down);
  node.addEventListener("pointermove", move);
  node.addEventListener("pointerup", up);
  node.addEventListener("pointercancel", cancel);
  node.addEventListener("lostpointercapture", cancel);
  view.addEventListener("keydown", key, true);
  view.addEventListener("blur", blur);
  return {
    update(next: SortOptions) { options = next; if (next.disabled) finish(); },
    destroy() {
      finish();
      node.removeEventListener("pointerdown", down);
      node.removeEventListener("pointermove", move);
      node.removeEventListener("pointerup", up);
      node.removeEventListener("pointercancel", cancel);
      node.removeEventListener("lostpointercapture", cancel);
      view.removeEventListener("keydown", key, true);
      view.removeEventListener("blur", blur);
    },
  };
}
