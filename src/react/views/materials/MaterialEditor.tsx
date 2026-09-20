import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { GripVertical } from "lucide-react";
import { inspectMaterialImage, inspectMaterialLibraryImage, saveMaterial, type MaterialDraft, type MaterialInspection, type MaterialVersionDraft } from "../../../lib/api/materials";
import { materialVersionDrafts, newMaterialVersion, duplicateMaterialVersion, moveMaterialVersion } from "../../../lib/utils/material-versions";
import { MATERIAL_IMAGE_EXTENSIONS, splitMaterialTags, mergeMaterialMetadata } from "../../../lib/utils/materials";
import { pointerSort, type SortPreview } from "../../../lib/utils/pointer-sort";
import { registerCloseGuard } from "../../../lib/stores/close-guard";
import { errorText } from "../../../lib/utils/format";
import { useLibrary } from "../../state/library";
import { useMaterials, materialCovers, materialVersionCovers, materialSaved, closeMaterialEditor } from "../../state/materials";
import { runTask } from "../../state/tasks";
import { Button, Checkbox, Input, Modal, Textarea } from "../../ui/controls";
import { MaterialImage } from "./MaterialImage";
import { MaterialGalleryPicker } from "./MaterialGalleryPicker";

export function MaterialEditor({ active = true }: { active?: boolean }) {
  const { editing: material, editingVersion, pendingPaths } = useMaterials.getState();
  const [draft, setDraft] = useState<MaterialDraft>(() => ({ id: material?.id ?? null, title: material?.title ?? "", text: material?.text ?? "", tags: [...material?.tags ?? []], imagePath: null, versions: materialVersionDrafts(material) }));
  const initial = useRef(JSON.stringify(draft));
  const [activeKey, setActiveKey] = useState(() => (draft.versions.find(v => v.id === editingVersion) ?? draft.versions[0]).key);
  const [imageTarget, setImageTarget] = useState<"cover" | "version">("cover");
  const [previews, setPreviews] = useState<Record<string, string>>({}); const urls = useRef(new Set<string>());
  const [inspection, setInspection] = useState<MaterialInspection | null>(null); const [metadataMode, setMetadataMode] = useState(false); const [sections, setSections] = useState<string[]>([]);
  const [busy, setBusy] = useState(false); const [error, setError] = useState(""); const [tagQuery, setTagQuery] = useState(""); const [picker, setPicker] = useState(false);
  const [confirm, setConfirm] = useState<{ title: string; text: string; action: () => void } | null>(null);
  const [sortPreview, setSortPreview] = useState<SortPreview | null>(null); const [listNode, setListNode] = useState<HTMLDivElement | null>(null);
  const current = draft.versions.find(version => version.key === activeKey) ?? draft.versions[0];
  const activeIndex = draft.versions.indexOf(current); const imageKey = imageTarget === "cover" ? "cover" : current.key;
  const combined = mergeMaterialMetadata(inspection?.sections ?? [], sections);
  const libraryTags = useLibrary(state => state.tags);
  const availableTags = [...new Set([...libraryTags.map(tag => tag.name), ...draft.tags])].filter(name => name.toLowerCase().includes(tagQuery.trim().toLowerCase()));
  const guard = useRef({ busy, dirty: false }); guard.current = { busy, dirty: JSON.stringify(draft) !== initial.current };
  const alive = useRef(true);
  const sortMove = useRef((key: string, to: number) => { setDraft(value => ({ ...value, versions: moveMaterialVersion(value.versions, value.versions.findIndex(v => v.key === key), to) })); });
  useEffect(() => { if (!listNode) return; const sort = pointerSort(listNode, { disabled: busy, onpreview: setSortPreview, onmove: (key, index) => sortMove.current(key, index) }); return () => sort.destroy(); }, [busy, listNode]);
  useEffect(() => {
    alive.current = true; const unregister = registerCloseGuard(() => guard.current.busy ? "素材正在读取或保存" : guard.current.dirty ? "素材有尚未确认保存的内容" : null);
    return () => { alive.current = false; unregister(); for (const url of urls.current) URL.revokeObjectURL(url); };
  }, []);
  const importStarted = useRef(false);
  useEffect(() => { if (pendingPaths[0] && !importStarted.current) { importStarted.current = true; void inspect(pendingPaths[0]); } }, []);
  function updateVersion(patch: Partial<MaterialVersionDraft>) { setDraft(value => ({ ...value, versions: value.versions.map(version => version.key === current.key ? { ...version, ...patch } : version) })); }
  function resetMetadata() { setInspection(null); setMetadataMode(false); setSections([]); }
  function selectVersion(key: string) { setActiveKey(key); resetMetadata(); }
  function move(from: number, to: number) { setDraft(value => ({ ...value, versions: moveMaterialVersion(value.versions, from, to) })); }
  function addVersion(duplicate: boolean) {
    const version = duplicate ? duplicateMaterialVersion(current) : newMaterialVersion(`版本 ${draft.versions.length + 1}`);
    if (duplicate && previews[current.key]) setPreviews(value => ({ ...value, [version.key]: previews[current.key] }));
    setDraft(value => ({ ...value, versions: [...value.versions, version] })); selectVersion(version.key); setImageTarget("version");
  }
  function applyImage(path: string, result: MaterialInspection, target: string) {
    if (!alive.current) return;
    const url = URL.createObjectURL(new Blob([new Uint8Array(result.preview)], { type: "image/png" })); urls.current.add(url);
    setPreviews(value => ({ ...value, [target]: url })); setInspection(result); setSections([]); setMetadataMode(false);
    setDraft(value => ({ ...value, title: value.title.trim() ? value.title : result.title, ...(target === "cover" ? { imagePath: path } : { versions: value.versions.map(version => version.key === target ? { ...version, imagePath: path, imageSourceId: null } : version) }) }));
  }
  async function inspect(path: string) { setBusy(true); setError(""); const target = imageKey; try { applyImage(path, await inspectMaterialImage(path), target); } catch (cause) { if (alive.current) setError(`无法读取图片：${errorText(cause)}`); } finally { if (alive.current) setBusy(false); } }
  async function chooseLocal() { try { const path = await open({ multiple: false, directory: false, title: "选择素材展示图", filters: [{ name: "图片", extensions: MATERIAL_IMAGE_EXTENSIONS }] }); if (typeof path === "string") await inspect(path); } catch (cause) { setError(errorText(cause)); } }
  async function chooseLibrary(id: number) { const target = imageKey; setPicker(false); setBusy(true); setError(""); try { const result = await inspectMaterialLibraryImage(id); applyImage(result.path, result.inspection, target); } catch (cause) { if (alive.current) setError(`无法读取图片：${errorText(cause)}`); } finally { if (alive.current) setBusy(false); } }
  function close() { if (busy) return; if (guard.current.dirty) setConfirm({ title: "取消编辑", text: "放弃尚未保存的素材内容？", action: closeMaterialEditor }); else closeMaterialEditor(); }
  function addTags() { setDraft(value => ({ ...value, tags: [...new Set([...value.tags, ...splitMaterialTags(tagQuery)])] })); setTagQuery(""); }
  async function save() {
    if (busy || (!draft.id && !draft.imagePath)) return;
    const directory = useLibrary.getState().snapshot?.dataDirectory;
    setBusy(true); setError(""); const value = { ...draft, text: draft.versions[0].text };
    try { const item = await runTask("保存素材", () => saveMaterial(value)); if (alive.current && directory === useLibrary.getState().snapshot?.dataDirectory) { initial.current = JSON.stringify(value); materialSaved(item); } }
    catch (cause) { if (alive.current) setError(`保存失败，内容已保留：${errorText(cause)}`); }
    finally { if (alive.current) setBusy(false); }
  }
  return <><Modal open={active && !picker} onClose={close} busy={busy} title={material ? "编辑素材" : "确认导入素材"} description={pendingPaths.length > 1 ? `之后还有 ${pendingPaths.length - 1} 张待确认` : "确认保存后修改才会生效"} width={900} footer={<><Button disabled={busy} onClick={close}>{pendingPaths.length > 1 ? "取消剩余导入" : "取消"}</Button><Button variant="primary" disabled={busy || (!draft.id && !draft.imagePath)} onClick={() => void save()}>{busy ? "处理中…" : material ? "保存修改" : pendingPaths.length > 1 ? "确认导入，继续下一张" : "确认导入"}</Button></>}>
    <div className="rm-editor"><div className="rm-cover-column"><div className="rm-image-switch"><Button disabled={busy} variant={imageTarget === "cover" ? "primary" : "ghost"} onClick={() => { setImageTarget("cover"); resetMetadata(); }}>固定封面</Button><Button disabled={busy} variant={imageTarget === "version" ? "primary" : "ghost"} onClick={() => { setImageTarget("version"); resetMetadata(); }}>版本图片</Button></div>
      <div className="rm-cover">{previews[imageKey] ? <img src={previews[imageKey]} alt="待保存图片" /> : imageTarget === "version" && current.imageSourceId ? <MaterialImage id={current.imageSourceId} loader={materialVersionCovers} alt={current.name} /> : previews.cover ? <img src={previews.cover} alt="固定封面" /> : material ? <MaterialImage id={material.id} loader={materialCovers} alt={material.title} /> : <span>选择一张展示图</span>}</div>
      <small>{imageTarget === "cover" ? "列表卡片始终显示此封面" : `${current.name} · ${current.imagePath || current.imageSourceId ? "独立图片" : "沿用固定封面"}`}</small>
      {imageTarget === "version" && (current.imagePath || current.imageSourceId) && <Button disabled={busy} onClick={() => { updateVersion({ imagePath: null, imageSourceId: null }); setPreviews(value => { const next = { ...value }; delete next[current.key]; return next; }); resetMetadata(); }}>改用固定封面</Button>}
      <Button variant="primary" disabled={busy} onClick={() => setPicker(true)}>去画廊选择</Button><Button disabled={busy} onClick={() => void chooseLocal()}>从本地文件选择</Button><small>支持 PNG、JPG、WebP 等图片。展示图随素材保存。</small>
    </div><fieldset disabled={busy} className="rm-fields"><label>名称<Input value={draft.title} onChange={event => setDraft(value => ({ ...value, title: event.target.value }))} placeholder="例如：黑金礼服" /></label>
      <label>Tag<Input value={tagQuery} placeholder="搜索或新建 Tag，多个用逗号分隔" onChange={event => setTagQuery(event.target.value)} onKeyDown={event => { if (event.key === "Enter" && !event.nativeEvent.isComposing) { event.preventDefault(); addTags(); } }} /></label>
      {tagQuery.trim() && <Button onClick={addTags}>添加输入的 Tag</Button>}<div className="rm-tags">{availableTags.map(name => <Button size="sm" key={name} aria-pressed={draft.tags.includes(name)} variant={draft.tags.includes(name) ? "primary" : "default"} onClick={() => setDraft(value => ({ ...value, tags: value.tags.includes(name) ? value.tags.filter(tag => tag !== name) : [...value.tags, name] }))}>{name}</Button>)}</div><small>{draft.tags.length ? `已选：${draft.tags.join("、")}` : "未选择 Tag"}</small>
      <div className="rm-version-heading"><strong>版本 · {draft.versions.length}</strong><small>拖动排序 · 第一项默认复制</small></div>
      <div className="rm-version-list" ref={setListNode} role="list" aria-label="素材版本">{draft.versions.map((version, index) => <div className={`rm-version-row ${version.key === current.key ? "is-active" : ""} ${sortPreview?.beforeKey === version.key ? "drop-before" : ""} ${sortPreview?.key === version.key ? "is-dragging" : ""}`} role="listitem" data-sort-key={version.key} key={version.key}><Button size="icon" variant="ghost" data-sort-handle aria-label={`拖动排序 ${version.name}`}><GripVertical size={14} /></Button><button className="rm-version-select" aria-pressed={version.key === current.key} onClick={() => selectVersion(version.key)}><span>{version.name || "未命名版本"}</span>{index === 0 && <small>默认</small>}</button><Button size="icon" variant="ghost" aria-label={`上移 ${version.name}`} disabled={index === 0} onClick={() => move(index, index - 1)}>↑</Button><Button size="icon" variant="ghost" aria-label={`下移 ${version.name}`} disabled={index === draft.versions.length - 1} onClick={() => move(index, index + 1)}>↓</Button></div>)}</div>
      <div className="rm-version-actions"><Button size="sm" disabled={draft.versions.length >= 128} onClick={() => addVersion(false)}>新增版本</Button><Button size="sm" disabled={draft.versions.length >= 128} onClick={() => addVersion(true)}>复制为新版本</Button>{activeIndex > 0 && <Button size="sm" onClick={() => move(activeIndex, 0)}>设为默认</Button>}<Button size="sm" variant="danger" disabled={draft.versions.length <= 1} onClick={() => setConfirm({ title: "删除版本", text: `删除版本「${current.name}」？保存素材后生效。`, action: () => { setDraft(value => ({ ...value, versions: value.versions.filter(version => version.key !== current.key) })); setActiveKey(draft.versions.find(version => version.key !== current.key)!.key); resetMetadata(); } })}>删除版本</Button></div>
      <label>版本名称<Input value={current.name} onChange={event => updateVersion({ name: event.target.value })} /></label>
      {inspection && <div className="rm-metadata">{inspection.warning && <p>{inspection.warning}</p>}{inspection.sections.length ? <><label className="rm-inline"><Checkbox checked={metadataMode} onCheckedChange={value => setMetadataMode(value === true)} />从元数据选择内容</label>{metadataMode && <>{inspection.sections.map(section => <label className="rm-inline" key={section.id}><Checkbox checked={sections.includes(section.id)} onCheckedChange={() => setSections(value => value.includes(section.id) ? value.filter(id => id !== section.id) : [...value, section.id])} />{section.label}</label>)}<Textarea aria-label="所选内容预览" readOnly value={combined} /><Button disabled={!sections.length} onClick={() => current.text && current.text !== combined ? setConfirm({ title: "填入元数据", text: "用所选元数据替换当前文本内容？", action: () => updateVersion({ text: combined }) }) : updateVersion({ text: combined })}>将所选内容填入下方文本</Button></>}</> : <p>未发现可提取的提示词文本，可在下方手动填写。</p>}</div>}
      <label>文本内容<Textarea className="rm-content" value={current.text} onChange={event => updateVersion({ text: event.target.value })} placeholder="此版本的提示词或其他文本。" /></label>
    </fieldset></div>{error && <p className="rm-error" role="alert">{error}</p>}
  </Modal>{active && picker && <MaterialGalleryPicker onClose={() => setPicker(false)} onChoose={id => void chooseLibrary(id)} />}
  <Modal open={active && !!confirm} onClose={() => setConfirm(null)} title={confirm?.title ?? "确认"} footer={<><Button onClick={() => setConfirm(null)}>取消</Button><Button variant="primary" onClick={() => { const action = confirm?.action; setConfirm(null); action?.(); }}>确认</Button></>}><p>{confirm?.text}</p></Modal>
  {sortPreview && <div className="rm-sort-floating" style={{ left: sortPreview.left, top: sortPreview.top, width: sortPreview.width }}><GripVertical size={14} />{draft.versions.find(version => version.key === sortPreview.key)?.name}</div>}
  </>;
}
