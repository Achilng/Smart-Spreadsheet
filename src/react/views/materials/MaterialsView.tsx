import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Copy, Edit3, PanelRightClose, PanelRightOpen, Trash2 } from "lucide-react";
import { galleryLayout, galleryCellPosition, galleryVisibleIndices } from "../../../lib/images/gallery-layout";
import { useWorkspace } from "../../state/workspace";
import { useTasks } from "../../state/tasks";
import { useMaterials, initializeMaterials, loadMaterialPage, reloadMaterials, MATERIAL_PAGE_SIZE, materialThumbnails, materialCardVersionImages, materialCovers, materialVersionCovers, toggleMaterialTag, setMaterialUntagged, clearMaterialFilters, createMaterial, chooseMaterialImages, beginMaterialImport, editMaterial, copyMaterial, requestDeleteMaterial, confirmDeleteMaterial } from "../../state/materials";
import { Button, Checkbox, Input, Modal, Slider } from "../../ui/controls";
import { MaterialImage } from "./MaterialImage";
import { MaterialEditor } from "./MaterialEditor";
import { MaterialCard } from "./MaterialCard";
import "./materials.css";

export function MaterialsView({ active = true }: { active?: boolean }) {
  const state = useMaterials(); const busy = useTasks(value => value.busy);
  const size = useWorkspace(value => value.galleryCardSize); const setSize = useWorkspace(value => value.setSize);
  const viewport = useRef<HTMLDivElement>(null); const [bounds, setBounds] = useState({ width: 0, height: 0 }); const [tagSearch, setTagSearch] = useState("");
  useEffect(() => { if (active) initializeMaterials(); }, [active, state.initialized]);
  useEffect(() => { if (!active || state.editorOpen || state.pendingDelete) useMaterials.setState({ openCardId: null }); }, [active, state.editorOpen, state.pendingDelete]);
  useEffect(() => { const handler = (event: Event) => { if (useWorkspace.getState().viewMode === "materials") beginMaterialImport((event as CustomEvent<string[]>).detail); }; window.addEventListener("material-path-drop", handler); return () => window.removeEventListener("material-path-drop", handler); }, []);
  useLayoutEffect(() => { const node = viewport.current; if (!node) return; const observer = new ResizeObserver(entries => { const rect = entries[0].contentRect; setBounds({ width: rect.width, height: rect.height }); }); observer.observe(node); node.scrollTop = useMaterials.getState().scrollTop; return () => observer.disconnect(); }, []);
  useLayoutEffect(() => { if (viewport.current && viewport.current.scrollTop !== state.scrollTop) viewport.current.scrollTop = state.scrollTop; }, [state.scrollTop, active, bounds.height]);
  const layout = galleryLayout(bounds.width, size, state.total);
  // Portrait cards keep the gallery's column positioning and virtual window.
  layout.imageHeight = Math.round(Math.max(1, layout.cardWidth - 12) * 1216 / 832);
  layout.cellHeight = layout.imageHeight + 58;
  layout.spacerHeight = layout.gridRows ? 32 + layout.gridRows * layout.cellHeight - 12 : 0;
  const visible = active ? galleryVisibleIndices(layout, state.scrollTop, bounds.height, state.total) : [];
  const first = visible[0] ?? 0; const last = visible.at(-1) ?? 0;
  useEffect(() => {
    if (!active) return;
    const ids = new Set<number>();
    const versionIds = new Set<number>();
    for (let page = Math.floor(first / MATERIAL_PAGE_SIZE); page <= Math.floor(last / MATERIAL_PAGE_SIZE); page++) void loadMaterialPage(page);
    for (const index of visible) {
      const item = state.pages.get(Math.floor(index / MATERIAL_PAGE_SIZE))?.[index % MATERIAL_PAGE_SIZE];
      if (!item) continue;
      ids.add(item.id);
      const version = item.versions.find(version => version.id === state.cardVersions[item.id]) ?? item.versions[0];
      if (version?.hasImage) versionIds.add(version.id);
    }
    materialThumbnails.retain(ids);
    materialCardVersionImages.retain(versionIds);
  }, [first, last, state.pages, state.cardVersions, active]);
  const tags = [...state.tags, ...state.selectedTags.filter(name => !state.tags.some(tag => tag.name === name)).map(name => ({ name, rowCount: 0 }))].filter(tag => tag.name.toLowerCase().includes(tagSearch.toLowerCase()));
  return <section className="rm-workspace" style={active ? undefined : { display: "none" }}><aside className="rm-sidebar"><div className="rm-sidebar-heading"><strong>素材筛选</strong><span>同时匹配</span></div><label className="rm-inline"><Checkbox checked={state.untagged} onCheckedChange={value => setMaterialUntagged(value === true)} />无 Tag 素材</label>{(state.search || state.selectedTags.length > 0 || state.untagged) && <Button size="sm" variant="ghost" onClick={clearMaterialFilters}>清除全部筛选</Button>}<div className="rm-sidebar-heading"><strong>Tag</strong><span>{state.selectedTags.length} 个已选</span></div><Input aria-label="搜索素材 Tag" placeholder="搜索 Tag…" value={tagSearch} onChange={event => setTagSearch(event.target.value)} />{state.tagError && <p className="rm-error" role="alert">{state.tagError}<Button size="sm" onClick={() => void reloadMaterials()}>重试</Button></p>}<div className="rm-tag-list">{tags.length ? tags.map(tag => <label className="rm-inline" key={tag.name}><Checkbox checked={state.selectedTags.includes(tag.name)} onCheckedChange={() => toggleMaterialTag(tag.name)} /><span>{tag.name}</span><small>{tag.rowCount}</small></label>) : <p>还没有 Tag。编辑素材时可以添加。</p>}</div></aside>
    <main className="rm-main"><header className="rm-header"><h1>素材 <small>{state.total}</small></h1><div className="rm-actions"><Slider aria-label="素材卡片大小" min={120} max={320} step={10} value={[size]} onValueChange={values => setSize(values[0])} /><Button disabled={busy || state.editorOpen} onClick={() => void chooseMaterialImages()}>导入图片</Button><Button variant="primary" disabled={busy || state.editorOpen} onClick={createMaterial}>新建素材</Button></div></header>
      {state.selectedTags.length > 0 && <p className="rm-filter-summary">Tag：{state.selectedTags.join("、")}</p>}{state.error && <div className="rm-error" role="alert">{state.error}<Button onClick={() => void reloadMaterials()}>重试</Button></div>}
      <div className="rm-viewport" ref={viewport} aria-busy={state.loading} onScroll={event => { if (active) useMaterials.setState({ scrollTop: event.currentTarget.scrollTop }); }} tabIndex={0} aria-label="素材列表">
        {state.total > 0 ? <div className="rm-grid" style={{ height: layout.spacerHeight }}>{visible.map(index => { const item = state.pages.get(Math.floor(index / MATERIAL_PAGE_SIZE))?.[index % MATERIAL_PAGE_SIZE]; const position = galleryCellPosition(index, layout); return <div key={item ? `material-${item.id}-${state.revision}` : `placeholder-${index}-${state.revision}`} className="rm-cell" style={{ left: position.x, top: position.y, width: layout.cardWidth, height: layout.cellHeight - 12 }}>{item ? <MaterialCard material={item} active={state.selected?.id === item.id} open={active && !state.editorOpen && !state.pendingDelete && state.openCardId === item.id} /> : <div className="r-image-placeholder" />}</div>; })}</div> : <div className="rm-empty"><h2>{state.loading ? "正在读取素材…" : state.error ? "素材读取失败" : state.search || state.selectedTags.length || state.untagged ? "没有匹配的素材" : "收藏你的第一份素材"}</h2>{!state.loading && !state.error && <p>新建素材可从图库选图，也可以将本地图片拖到这里导入。</p>}</div>}
      </div>
    </main><MaterialDetailPanel active={active && !state.editorOpen && !state.pendingDelete} />
    {state.editorOpen && <MaterialEditor active={active} key={state.editorKey} />}
    <Modal open={active && state.pendingDelete !== null} busy={state.deleting} title={`删除素材「${state.pendingDelete?.title ?? ""}」？`} onClose={() => useMaterials.setState({ pendingDelete: null, deleteError: "" })} footer={<><Button disabled={state.deleting} onClick={() => useMaterials.setState({ pendingDelete: null, deleteError: "" })}>取消</Button><Button variant="danger" disabled={state.deleting} onClick={() => void confirmDeleteMaterial()}>{state.deleting ? "正在删除…" : "删除素材"}</Button></>}><p>该素材的全部 {state.pendingDelete?.versions.length ?? 0} 个版本、文本及展示图将一并删除，原始图片不会被修改。</p><p>删除后无法通过 Ctrl+Z 恢复。</p>{state.deleteError && <p className="rm-error" role="alert">{state.deleteError}</p>}</Modal>
  </section>;
}
export function MaterialDetailPanel({ active = true }: { active?: boolean }) {
  const material = useMaterials(state => state.selected); const open = useMaterials(state => state.detailOpen); const revision = useMaterials(state => state.revision); const busy = useTasks(state => state.busy);
  if (!open) return <aside className="rm-detail-collapsed"><Button size="icon" variant="ghost" aria-label="展开素材详情" onClick={() => useMaterials.setState({ detailOpen: true })}><PanelRightOpen size={16} /></Button></aside>;
  return <aside className="rm-detail"><header><strong>素材详情</strong><Button size="icon" variant="ghost" aria-label="收起素材详情" onClick={() => useMaterials.setState({ detailOpen: false })}><PanelRightClose size={16} /></Button></header>{material ? <div className="rm-detail-scroll" key={`${material.id}-${revision}`}><h2 title={material.title}>{material.title}</h2><MaterialImage id={material.id} loader={materialCovers} alt={material.title} expandable active={active} /><div className="rm-actions"><Button size="sm" disabled={busy} onClick={() => editMaterial(material)}><Edit3 size={13} />编辑</Button><Button size="sm" onClick={() => void copyMaterial(material)}><Copy size={13} />复制</Button><Button size="sm" variant="danger" disabled={busy} onClick={() => requestDeleteMaterial(material)}><Trash2 size={13} />删除</Button></div><div className="rm-tags">{material.tags.map(tag => <span className="rm-tag" key={tag}>{tag}</span>)}</div>{(material.versions.length ? material.versions : [{ id: 0, name: "默认版本", text: material.text, hasImage: false }]).map((version, index) => <section className="rm-version-detail" key={version.id}><div className="rm-version-heading"><h3>{version.name} {index === 0 && <small>默认</small>}</h3><div className="rm-actions"><Button variant="ghost" size="sm" disabled={busy} onClick={() => editMaterial(material, version.id)}>编辑</Button><Button variant="ghost" size="sm" disabled={!version.text} onClick={() => void copyMaterial(material, version.text)}>复制</Button></div></div>{version.hasImage && <MaterialImage id={version.id} loader={materialVersionCovers} alt={version.name} expandable active={active} />}<pre>{version.text || "—"}</pre></section>)}</div> : <div className="rm-empty"><p>选择一份素材查看详情</p></div>}</aside>;
}
