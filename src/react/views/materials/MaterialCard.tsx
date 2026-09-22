import { memo, useEffect, useId, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import type { Material } from "../../../lib/api/materials";
import { copyMaterial, materialCardVersionImages, materialThumbnails, useMaterials } from "../../state/materials";
import { useImage } from "../../ui/use-image";
import "./material-card.css";

/** A version choice is browsing state, independent of the saved default version. */
export const MaterialCard = memo(function MaterialCard({ material, active, open }: { material: Material; active: boolean; open: boolean }) {
  const chosenId = useMaterials(state => state.cardVersions[material.id]);
  const versions = material.versions.length ? material.versions : [{ id: 0, name: "默认版本", text: material.text, hasImage: false }];
  const version = versions.find(item => item.id === chosenId) ?? versions[0];
  const image = useImage(version.hasImage ? materialCardVersionImages : materialThumbnails, version.hasImage ? version.id : material.id);
  const root = useRef<HTMLElement>(null);
  const face = useRef<HTMLDivElement>(null);
  const hit = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const frame = useRef(0);
  const copySequence = useRef(0);
  const feedbackTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const [copyFeedback, setCopyFeedback] = useState({ message: "", visible: false });
  const [query, setQuery] = useState("");
  const panelId = useId();
  const options = versions.filter(item => item.name.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));

  function copyVersionText(text: string) {
    const sequence = ++copySequence.current;
    clearTimeout(feedbackTimer.current);
    setCopyFeedback({ message: "", visible: false });
    void copyMaterial(material, text, (_message, tone) => {
      // Ignore a late clipboard response after another copy or card unmount.
      if (sequence !== copySequence.current) return;
      setCopyFeedback({ message: tone === "error" ? (text ? "复制失败，请重试" : "暂无文本，请先编辑") : "✓ 复制成功", visible: true });
      feedbackTimer.current = setTimeout(() => setCopyFeedback(value => ({ ...value, visible: false })), 1800);
    });
  }

  function resetTilt() {
    cancelAnimationFrame(frame.current);
    const node = face.current;
    if (!node) return;
    node.classList.remove("is-hovering");
    node.style.setProperty("--rx", "0deg");
    node.style.setProperty("--ry", "0deg");
  }
  function close(focus = false) {
    useMaterials.setState(state => state.openCardId === material.id ? { openCardId: null } : {});
    if (focus) hit.current?.focus({ preventScroll: true });
  }
  function toggle() {
    useMaterials.setState(state => ({ openCardId: state.openCardId === material.id ? null : material.id }));
  }
  useEffect(() => {
    if (!open) return;
    resetTilt();
    setQuery("");
    const focusFrame = requestAnimationFrame(() => {
      const target = panel.current?.querySelector<HTMLElement>(versions.length > 8 ? "input" : '[aria-pressed="true"]');
      target?.focus({ preventScroll: true });
    });
    const outside = (event: globalThis.PointerEvent) => { if (!root.current?.contains(event.target as Node)) close(); };
    document.addEventListener("pointerdown", outside);
    return () => { cancelAnimationFrame(focusFrame); document.removeEventListener("pointerdown", outside); };
  }, [open]);
  useEffect(() => {
    const reduced = matchMedia("(prefers-reduced-motion: reduce)");
    window.addEventListener("blur", resetTilt);
    reduced.addEventListener("change", resetTilt);
    return () => { cancelAnimationFrame(frame.current); clearTimeout(feedbackTimer.current); copySequence.current++; window.removeEventListener("blur", resetTilt); reduced.removeEventListener("change", resetTilt); };
  }, []);

  function move(event: PointerEvent<HTMLDivElement>) {
    if (open || event.pointerType === "touch" || matchMedia("(prefers-reduced-motion: reduce)").matches || !matchMedia("(hover: hover)").matches) return;
    const bounds = root.current!.getBoundingClientRect();
    const x = Math.max(0, Math.min(1, (event.clientX - bounds.left) / bounds.width));
    const y = Math.max(0, Math.min(1, (event.clientY - bounds.top) / (face.current?.offsetHeight || bounds.height)));
    cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(() => {
      const node = face.current;
      if (!node) return;
      node.classList.add("is-hovering");
      node.style.setProperty("--rx", `${(.5 - y) * 4}deg`);
      node.style.setProperty("--ry", `${(x - .5) * 5}deg`);
      node.style.setProperty("--mx", `${x * 100}%`);
      node.style.setProperty("--my", `${y * 100}%`);
    });
  }
  function navigate(event: KeyboardEvent<HTMLDivElement>) {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    if (event.target instanceof HTMLInputElement && ["Home", "End"].includes(event.key)) return;
    const buttons = Array.from(panel.current?.querySelectorAll<HTMLButtonElement>(".rm-look-option") ?? []);
    if (!buttons.length) return;
    event.preventDefault();
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.key === "Home" ? 0 : event.key === "End" ? buttons.length - 1 : event.key === "ArrowDown" ? (index + 1) % buttons.length : (index < 0 ? buttons.length - 1 : (index - 1 + buttons.length) % buttons.length);
    buttons[next].focus({ preventScroll: true });
    buttons[next].scrollIntoView({ block: "nearest" });
  }
  return <article ref={root} className={`rm-portrait-card${active ? " is-active" : ""}${open ? " is-open" : ""}`} aria-label={material.title}
    onKeyDown={event => { if (event.key === "Escape" && open) { event.preventDefault(); event.stopPropagation(); close(true); } }}>
    <div className="rm-portrait-stack">
      <div ref={face} className="rm-portrait-face" onPointerMove={move} onPointerLeave={resetTilt} onPointerCancel={resetTilt}
        onContextMenu={event => { event.preventDefault(); toggle(); }}>
        <div className="rm-portrait-art">
          {image.url ? <img src={image.url} alt={`${material.title} · ${version.name}`} draggable={false} /> : <div className="rm-portrait-placeholder">{image.error ? "图片暂时无法显示" : "正在加载图片…"}</div>}
          <div className="rm-portrait-shade" /><div className="rm-portrait-glare" />
        </div>
        <button ref={hit} type="button" className="rm-portrait-hit" aria-label={`选择素材 ${material.title}，当前版本 ${version.name}`} aria-pressed={active} aria-expanded={open} aria-controls={panelId}
          onClick={() => { if (open) close(true); useMaterials.setState({ selected: material }); }}
          onDoubleClick={() => copyVersionText(version.text)}
          onKeyDown={event => { if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) { event.preventDefault(); toggle(); } }} />
        <div className="rm-portrait-identity">
          <h3 className="rm-portrait-name">{material.title}</h3>
          <span className="rm-portrait-count" role="img" aria-label={`共 ${versions.length} 个版本`}>
            <span className="rm-portrait-stack-icon" aria-hidden="true" />
            <span aria-hidden="true">{versions.length}</span>
          </span>
        </div>
        <div className={`rm-portrait-toast${copyFeedback.visible ? " is-visible" : ""}`} role="status" aria-live="polite" aria-atomic="true" aria-hidden={!copyFeedback.visible}>{copyFeedback.message}</div>
        {image.error && <button className="rm-portrait-retry" onClick={() => image.retry()}>重试图片</button>}
      </div>
    </div>
    <div ref={panel} id={panelId} className="rm-look-panel" inert={!open} aria-hidden={!open} onKeyDown={navigate}>
      {versions.length > 8 && <input className="rm-look-search" aria-label="搜索版本" placeholder="搜索版本…" value={query} onChange={event => setQuery(event.target.value)} onKeyDown={event => { if (event.key === "Enter") panel.current?.querySelector<HTMLButtonElement>(".rm-look-option")?.click(); }} />}
      <div className="rm-look-options" role="group" aria-label="可选版本">{options.map(item => <button key={item.id} type="button" className="rm-look-option" aria-pressed={item.id === version.id} onClick={() => {
        useMaterials.setState(state => ({ cardVersions: { ...state.cardVersions, [material.id]: item.id } }));
        copyVersionText(item.text);
        close(true);
      }}>{item.name}</button>)}{!options.length && <p className="rm-look-empty">没有匹配的版本</p>}</div>
      <button type="button" className="rm-look-close" onClick={() => close(true)}>收起 ↑</button>
    </div>
  </article>;
});
