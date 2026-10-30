import { useEffect, useRef, useState, type ReactNode } from "react";
import { ContextMenu } from "radix-ui";
import { flushSync } from "react-dom";
import { create } from "zustand";
import { artistWhitelistSelection, updateArtistWhitelist, type ArtistTextSelection, type ArtistWhitelistSelection } from "../../lib/api/local-llm";
import { errorText } from "../../lib/utils/format";
import { notify } from "../state/notices";
import { runTask, useTasks } from "../state/tasks";

type TextSelection = ArtistTextSelection & { element: HTMLElement };
const useTextSelection = create<{ selection: TextSelection | null }>(() => ({ selection: null }));

function readSelection(): TextSelection | null {
  const selection = window.getSelection();
  if (!selection || selection.isCollapsed || selection.rangeCount !== 1) return null;
  const range = selection.getRangeAt(0);
  const ancestor = range.commonAncestorContainer;
  const element = (ancestor instanceof Element ? ancestor : ancestor.parentElement)?.closest<HTMLElement>("[data-artist-text]");
  if (!element || !element.contains(range.startContainer) || !element.contains(range.endContainer)) return null;
  const before = range.cloneRange();
  before.selectNodeContents(element);
  before.setEnd(range.startContainer, range.startOffset);
  const start = before.toString().length;
  return { element, text: element.textContent ?? "", start, end: start + range.toString().length };
}

/** One selection listener per window, independent of how many rows are visible. */
export function ArtistSelectionEvents() {
  useEffect(() => {
    const update = () => useTextSelection.setState({ selection: readSelection() });
    document.addEventListener("selectionchange", update);
    return () => { document.removeEventListener("selectionchange", update); useTextSelection.setState({ selection: null }); };
  }, []);
  return null;
}

type MenuState = { selection: ArtistTextSelection; revision: number; preview?: ArtistWhitelistSelection; error?: string };

export function ArtistWhitelistMenu({ text, revision, onChanged, children }: { text: string; revision: number; onChanged: () => void; children: ReactNode }) {
  const element = useRef<HTMLSpanElement>(null);
  const selected = useTextSelection(state => state.selection?.element === element.current ? state.selection : null);
  const busy = useTasks(state => state.busy);
  const request = useRef(0);
  const [menu, setMenu] = useState<MenuState | null>(null);
  useEffect(() => () => { request.current++; }, []);
  const apply = async (remove: boolean) => {
    if (!menu?.preview || menu.revision !== revision || useTasks.getState().busy) return;
    try {
      const count = await runTask("更新画师串白名单", () => updateArtistWhitelist(menu.preview!.libraryId, menu.selection, remove));
      onChanged();
      notify(count ? `已${remove ? "从白名单移除" : "加入白名单"} ${count} 个 tag，对当前资料库所有图片生效。` : "白名单已是最新状态。");
    } catch (error) { notify(`更新白名单失败：${errorText(error)}`, "error"); }
  };
  const stale = menu && (menu.revision !== revision || menu.selection.text !== text);
  return <ContextMenu.Root onOpenChange={open => { if (!open) request.current++; }}>
    <ContextMenu.Trigger asChild disabled={!selected}>
      <span ref={element} data-artist-text="" className="r-artist-text" onPointerDownCapture={event => {
        // selectionchange may still be queued when the user right-clicks quickly.
        if (event.button === 2) flushSync(() => useTextSelection.setState({ selection: readSelection() }));
      }} onClick={event => {
        // Dragging text in a table or result summary must not select a row or toggle details.
        if (readSelection()?.element === element.current) { event.preventDefault(); event.stopPropagation(); }
      }} onContextMenu={event => {
        const current = readSelection();
        if (!current || current.element !== element.current) { if (selected) event.preventDefault(); return; }
        event.stopPropagation();
        const selection = { text: current.text, start: current.start, end: current.end };
        const id = ++request.current;
        setMenu({ selection, revision });
        void artistWhitelistSelection(selection).then(preview => {
          if (id === request.current) setMenu({ selection, revision, preview });
        }, error => { if (id === request.current) setMenu({ selection, revision, error: errorText(error) }); });
      }}>{children}</span>
    </ContextMenu.Trigger>
    <ContextMenu.Portal><ContextMenu.Content className="r-menu r-artist-whitelist-menu" collisionPadding={8} onContextMenu={event => event.stopPropagation()}>
      {stale ? <ContextMenu.Item className="r-menu-item" disabled>白名单或资料库已变化，请重新选择</ContextMenu.Item>
        : menu?.error ? <ContextMenu.Item className="r-menu-item" disabled>{menu.error}</ContextMenu.Item>
        : !menu?.preview ? <ContextMenu.Item className="r-menu-item" disabled>正在读取白名单…</ContextMenu.Item>
        : <>
          {menu.preview.addTags.length > 0 && <ContextMenu.Item className="r-menu-item" disabled={busy} onSelect={() => void apply(false)}>将这些tag加入白名单（所有图片生效）</ContextMenu.Item>}
          {menu.preview.removeTags.length > 0 && <ContextMenu.Item className="r-menu-item" disabled={busy} onSelect={() => void apply(true)}>取消加入白名单（所有图片生效）</ContextMenu.Item>}
        </>}
    </ContextMenu.Content></ContextMenu.Portal>
  </ContextMenu.Root>;
}
