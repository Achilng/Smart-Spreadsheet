import { memo, useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import { Images, Layers, X } from "lucide-react";
import type { Material } from "../../../lib/api/materials";
import { materialAlbum } from "../../../lib/utils/material-album";
import { copyMaterial, materialCardVersionImages, materialThumbnails, useMaterials } from "../../state/materials";
import { AlbumStack } from "../../ui/AlbumStack";
import { MaterialImage } from "./MaterialImage";
import "./material-card.css";

export const MaterialCard = memo(function MaterialCard({ material, active, open }: { material: Material; active: boolean; open: boolean }) {
  const chosenId = useMaterials(state => state.cardVersions[material.id]);
  const { versions, version, images, previews } = materialAlbum(material, chosenId);
  const root = useRef<HTMLElement>(null), hit = useRef<HTMLButtonElement>(null), panel = useRef<HTMLDivElement>(null);
  const [query, setQuery] = useState("");
  const panelId = useId();
  const options = versions.filter(item => item.name.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));
  function close(focus = false) {
    useMaterials.setState(state => state.openCardId === material.id ? { openCardId: null } : {});
    if (focus) hit.current?.focus({ preventScroll: true });
  }
  function toggle() { useMaterials.setState(state => ({ openCardId: state.openCardId === material.id ? null : material.id })); }
  useEffect(() => {
    if (!open) return;
    setQuery("");
    const frame = requestAnimationFrame(() => panel.current?.querySelector<HTMLElement>(versions.length > 8 ? "input" : '[aria-pressed="true"]')?.focus({ preventScroll: true }));
    const outside = (event: globalThis.PointerEvent) => { if (!root.current?.contains(event.target as Node)) close(); };
    document.addEventListener("pointerdown", outside);
    return () => { cancelAnimationFrame(frame); document.removeEventListener("pointerdown", outside); };
  }, [open]);
  function navigate(event: KeyboardEvent<HTMLDivElement>) {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    if (event.target instanceof HTMLInputElement && ["Home", "End"].includes(event.key)) return;
    const buttons = Array.from(panel.current?.querySelectorAll<HTMLButtonElement>(".rm-look-option") ?? []);
    if (!buttons.length) return;
    event.preventDefault();
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.key === "Home" ? 0 : event.key === "End" ? buttons.length - 1 : event.key === "ArrowDown" ? (index + 1) % buttons.length : (index < 0 ? buttons.length - 1 : (index - 1 + buttons.length) % buttons.length);
    buttons[next].focus({ preventScroll: true }); buttons[next].scrollIntoView({ block: "nearest" });
  }
  return <article ref={root} className={`rm-album-card${open ? " is-open" : ""}`} aria-label={material.title}
    onKeyDown={event => { if (event.key === "Escape" && open) { event.preventDefault(); event.stopPropagation(); close(true); } }}>
    <button ref={hit} type="button" className="r-album-trigger" aria-label={`选择素材 ${material.title}，当前版本 ${version.name}`} aria-pressed={active}
      onClick={() => { if (open) close(); useMaterials.setState({ selected: material, detailOpen: true }); }}
      onDoubleClick={() => void copyMaterial(material, version.text)}
      onContextMenu={event => { event.preventDefault(); toggle(); }}
      onKeyDown={event => { if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) { event.preventDefault(); toggle(); } }}>
      <AlbumStack images={previews.map(image => <MaterialImage passive key={`${image.kind}-${image.id}`} id={image.id} loader={image.kind === "cover" ? materialThumbnails : materialCardVersionImages} alt={`${material.title} · ${image.label}`} />)} />
      <span className="r-album-title">{material.title}</span><span className="rm-album-tags" title={material.tags.join(" · ")}>{material.tags.join(" · ") || "无 Tag"}</span>
    </button>
    <div className="rm-album-meta"><span><Images size={14} />{images.length} 张图片</span><button type="button" aria-expanded={open} aria-controls={panelId} aria-label={`选择 ${material.title} 的版本`} onClick={toggle}><Layers size={14} />{versions.length} 个版本</button></div>
    {open && <div ref={panel} id={panelId} className="rm-look-panel" onKeyDown={navigate}>
      <header><strong>切换并复制</strong><button type="button" aria-label="收起版本" onClick={() => close(true)}><X size={16} /></button></header>
      {versions.length > 8 && <input className="rm-look-search" aria-label="搜索版本" placeholder="搜索版本…" value={query} onChange={event => setQuery(event.target.value)} onKeyDown={event => { if (event.key === "Enter") panel.current?.querySelector<HTMLButtonElement>(".rm-look-option")?.click(); }} />}
      <div className="rm-look-options" role="group" aria-label="可选版本">{options.map(item => <button key={item.id} type="button" className="rm-look-option" aria-pressed={item.id === version.id} onClick={() => {
        useMaterials.setState(state => ({ cardVersions: { ...state.cardVersions, [material.id]: item.id } }));
        void copyMaterial(material, item.text); close(true);
      }}>{item.name}</button>)}{!options.length && <p>没有匹配的版本</p>}</div>
    </div>}
  </article>;
});
