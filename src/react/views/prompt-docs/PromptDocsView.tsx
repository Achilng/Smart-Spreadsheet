import { useEffect, useRef, useState } from "react";
import { Editor } from "@tiptap/core";
import FileHandler from "@tiptap/extension-file-handler";
import Image from "@tiptap/extension-image";
import StarterKit from "@tiptap/starter-kit";
import { open } from "@tauri-apps/plugin-dialog";
import { Bold, Check, Copy, FileText, ImagePlus, Italic, List, LoaderCircle, Plus, Redo2, RotateCw, Trash2, Undo2 } from "lucide-react";
import { importPromptDocImageBytes, importPromptDocImageFromPath } from "../../../lib/api/prompt-docs";
import { errorText } from "../../../lib/utils/format";
import { Button, Input, Modal } from "../../ui/controls";
import { notify } from "../../state/notices";
import { runTask, useTasks } from "../../state/tasks";
import { useLibrary } from "../../state/library";
import { editPromptDoc, flushPromptDocSave, initializePromptDocs, registerPromptDocAsset, removeActivePromptDoc, switchPromptDoc, usePromptDocs } from "../../state/prompt-docs";
import "./prompt-docs.css";

const extensions = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"];
const mimeTypes = ["image/png", "image/jpeg", "image/webp", "image/gif", "image/bmp", "image/tiff"];

export function PromptDocsView() {
  const state = usePromptDocs();
  const busy = useTasks(store => store.busy);
  const directory = useLibrary(store => store.snapshot?.dataDirectory);
  const host = useRef<HTMLDivElement>(null);
  const editorRef = useRef<Editor | null>(null);
  const [, renderSelection] = useState(0);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [discardOpen, setDiscardOpen] = useState(false);
  const discardResolver = useRef<((answer: boolean) => void) | null>(null);
  const [copied, setCopied] = useState(false);
  const copyTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const mounted = useRef(false);
  const locked = state.loading || busy || state.directory !== (directory ?? null);

  async function importImages(files: File[] | string[], position?: number): Promise<void> {
    const current = usePromptDocs.getState();
    if (current.loading || useTasks.getState().busy) return;
    if (!current.active) { notify("请先创建或选择一个提示词文档。", "error"); return; }
    const accepted = files.filter(file => typeof file === "string"
      ? extensions.includes(file.split(".").pop()?.toLowerCase() ?? "") : mimeTypes.includes(file.type));
    if (!accepted.length) return;
    const id = current.active.id;
    const editor = editorRef.current;
    if (!editor) return;
    usePromptDocs.setState({ loading: true });
    try {
      await runTask("插入文档图片", async () => {
        let nextPosition = position;
        for (const file of accepted) {
          const asset = typeof file === "string" ? await importPromptDocImageFromPath(id, file)
            : await importPromptDocImageBytes(id, file.name || "clipboard.png", Array.from(new Uint8Array(await file.arrayBuffer())));
          if (usePromptDocs.getState().active?.id !== id) break;
          const src = registerPromptDocAsset(asset);
          // The view may have unmounted while an image was importing. Preserve
          // the imported image in the draft without touching a destroyed editor.
          if (!editor.isDestroyed) {
            if (typeof nextPosition === "number") {
              const safePosition = Math.max(0, Math.min(nextPosition, editor.state.doc.content.size));
              editor.chain().focus().insertContentAt(safePosition, { type: "image", attrs: { src } }).run();
              nextPosition = safePosition + 1;
            } else editor.chain().focus().setImage({ src }).run();
          } else {
            const draft = usePromptDocs.getState();
            editPromptDoc({ content: { ...draft.content, content: [...draft.content.content ?? [], { type: "image", attrs: { src } }] } });
            usePromptDocs.setState(store => ({ documentVersion: store.documentVersion + 1 }));
          }
        }
      });
    } catch (error) { notify(`插入图片失败：${errorText(error)}`, "error"); }
    finally { usePromptDocs.setState({ loading: false }); }
  }

  useEffect(() => {
    mounted.current = true;
    const current = usePromptDocs.getState();
    const editor = new Editor({
      element: host.current!, editable: Boolean(current.active) && !current.loading,
      content: current.content,
      extensions: [
        StarterKit.configure({ heading: { levels: [1, 2, 3] } }),
        Image.configure({ allowBase64: false, resize: { enabled: true, minWidth: 80, minHeight: 80, alwaysPreserveAspectRatio: true } }),
        FileHandler.configure({ allowedMimeTypes: mimeTypes,
          onPaste: (_editor, files) => { void importImages(files); },
          onDrop: (_editor, files, position) => { void importImages(files, position); },
        }),
      ],
      editorProps: { attributes: { "aria-label": "提示词文档正文", role: "textbox", "aria-multiline": "true" } },
      onUpdate: ({ editor: instance }) => {
        editPromptDoc({ content: instance.getJSON(), plainText: instance.getText({ blockSeparator: "\n" }) });
        renderSelection(value => value + 1);
      },
      onSelectionUpdate: () => renderSelection(value => value + 1),
    });
    editorRef.current = editor;
    const pathDrop = (event: Event) => { const paths = (event as CustomEvent<string[]>).detail; if (Array.isArray(paths)) void importImages(paths); };
    window.addEventListener("prompt-doc-path-drop", pathDrop);
    return () => {
      mounted.current = false;
      discardResolver.current?.(false);
      discardResolver.current = null;
      window.removeEventListener("prompt-doc-path-drop", pathDrop);
      clearTimeout(copyTimer.current);
      void flushPromptDocSave();
      editor.destroy();
      editorRef.current = null;
    };
  }, [state.documentVersion]);

  useEffect(() => { void initializePromptDocs(); }, [directory]);
  useEffect(() => { editorRef.current?.setEditable(Boolean(state.active) && !locked, false); }, [state.active?.id, locked]);

  const confirmDiscard = () => new Promise<boolean>(resolve => { discardResolver.current = resolve; setDiscardOpen(true); });
  const resolveDiscard = (answer: boolean) => { setDiscardOpen(false); discardResolver.current?.(answer); discardResolver.current = null; };
  async function chooseImages(): Promise<void> {
    try {
      const result = await open({ multiple: true, directory: false, title: "选择要插入的图片", filters: [{ name: "图片", extensions }] });
      if (result) await importImages(Array.isArray(result) ? result : [result]);
    } catch (error) { notify(errorText(error), "error"); }
  }
  async function copy(): Promise<void> {
    try {
      await navigator.clipboard.writeText(editorRef.current?.getText({ blockSeparator: "\n" }) ?? state.plainText);
      if (mounted.current) { setCopied(true); clearTimeout(copyTimer.current); copyTimer.current = setTimeout(() => setCopied(false), 1800); }
      notify("提示词文本已复制。");
    } catch (error) { notify(`复制失败：${errorText(error)}`, "error"); }
  }
  const keyword = state.search.trim().toLocaleLowerCase();
  const docs = state.docs.filter(doc => !keyword || `${doc.title}\n${doc.plainText}`.toLocaleLowerCase().includes(keyword));
  const editor = editorRef.current;
  const status = !state.active ? "" : state.saveState === "saving" ? "保存中…" : state.saveState === "dirty" ? "有未保存修改" : state.saveState === "error" ? "保存失败" : "已保存";

  return <section className="r-prompt-docs" aria-label="提示词文档">
    <aside className="r-pd-list">
      <header><h2>提示词文档</h2><Button variant="primary" disabled={locked} onClick={() => void switchPromptDoc(null, confirmDiscard)}><Plus size={14} />新建</Button></header>
      <Input aria-label="搜索提示词文档" placeholder="搜索标题和正文…" value={state.search} onChange={event => usePromptDocs.setState({ search: event.target.value })} />
      <div className="r-pd-items" aria-label="文档列表">
        {state.error && <div className="r-pd-error" role="alert"><p>{state.error}</p><Button disabled={locked} onClick={() => void initializePromptDocs()}><RotateCw size={14} />重新加载</Button></div>}
        {!state.error && !docs.length && <p className="r-pd-list-empty">{state.loading ? "正在加载文档…" : keyword ? "没有匹配的文档" : "还没有提示词文档"}</p>}
        {docs.map(doc => <button key={doc.id} type="button" className={`r-pd-item ${state.active?.id === doc.id ? "is-active" : ""}`} aria-current={state.active?.id === doc.id ? "page" : undefined} disabled={locked} onClick={() => void switchPromptDoc(doc.id, confirmDiscard)}>
          <span className="r-pd-name" title={doc.title}>{doc.title}</span><time>{doc.updatedAt}</time>{doc.plainText.trim() && <span className="r-pd-snippet">{doc.plainText}</span>}
        </button>)}
      </div>
    </aside>
    <main className="r-pd-editor" aria-busy={state.loading}>
      {state.loading && <div className="r-pd-progress" />}
      <div className="r-pd-titlebar">
        <Input className="r-pd-title" aria-label="文档标题" value={state.title} placeholder="未命名文档" disabled={!state.active || locked} onChange={event => editPromptDoc({ title: event.target.value })} />
        <span className={`r-pd-status ${state.saveState === "error" ? "is-error" : ""}`} role="status">{state.saveState === "saving" ? <LoaderCircle className="r-pd-spin" size={12} /> : state.saveState === "saved" ? <Check size={12} /> : null}{status}</span>
        {state.saveState === "error" && <Button size="sm" onClick={() => void flushPromptDocSave()} disabled={locked}>重试保存</Button>}
        <Button disabled={!state.active || locked} onClick={() => void copy()} title="复制全文纯文本" aria-label="复制全文纯文本">{copied ? <Check size={14} /> : <Copy size={14} />}<span className="r-pd-copy-label">复制全文纯文本</span></Button>
        <Button variant="danger" disabled={!state.active || locked} onClick={() => setDeleteOpen(true)} title="删除文档" aria-label="删除文档"><Trash2 size={14} /><span className="r-pd-delete-label">删除</span></Button>
      </div>
      <div className="r-pd-toolbar" role="toolbar" aria-label="提示词文档工具栏">
        <Button disabled={!state.active || locked} aria-pressed={editor?.isActive("paragraph") ?? false} onClick={() => editor?.chain().focus().setParagraph().run()}>正文</Button>
        <Button disabled={!state.active || locked} aria-pressed={editor?.isActive("heading", { level: 2 }) ?? false} onClick={() => editor?.chain().focus().toggleHeading({ level: 2 }).run()}>标题</Button>
        <Button disabled={!state.active || locked} aria-label="粗体" aria-pressed={editor?.isActive("bold") ?? false} onClick={() => editor?.chain().focus().toggleBold().run()}><Bold size={14} /></Button>
        <Button disabled={!state.active || locked} aria-label="斜体" aria-pressed={editor?.isActive("italic") ?? false} onClick={() => editor?.chain().focus().toggleItalic().run()}><Italic size={14} /></Button>
        <Button disabled={!state.active || locked} aria-pressed={editor?.isActive("bulletList") ?? false} onClick={() => editor?.chain().focus().toggleBulletList().run()}><List size={14} />列表</Button>
        <Button disabled={!state.active || locked} onClick={() => void chooseImages()}><ImagePlus size={14} />插入图片</Button>
        <span className="r-pd-toolbar-separator" />
        <Button aria-label="撤销文档编辑" disabled={!state.active || locked || !editor?.can().undo()} onClick={() => editor?.chain().focus().undo().run()}><Undo2 size={14} /></Button>
        <Button aria-label="重做文档编辑" disabled={!state.active || locked || !editor?.can().redo()} onClick={() => editor?.chain().focus().redo().run()}><Redo2 size={14} /></Button>
      </div>
      <div className="r-pd-shell"><div className={`r-pd-surface ${!state.active ? "is-disabled" : ""}`}>
        <div ref={host} className="r-pd-content" />
        {!state.active && <div className="r-pd-empty"><FileText size={30} /><h3>创建一个提示词文档</h3><p>整理提示词，插入图片，修改后自动保存。</p><Button variant="primary" disabled={locked} onClick={() => void switchPromptDoc(null, confirmDiscard)}><Plus size={14} />新建文档</Button></div>}
      </div></div>
    </main>
    <Modal open={deleteOpen} onClose={() => setDeleteOpen(false)} title="删除提示词文档" description={`将删除「${state.title || "未命名文档"}」及其中插入的图片，此操作不可撤销。`} busy={state.loading}
      footer={<><Button disabled={state.loading} onClick={() => setDeleteOpen(false)}>取消</Button><Button variant="danger" disabled={state.loading} onClick={() => { void removeActivePromptDoc().then(() => { if (mounted.current) setDeleteOpen(false); }); }}>确认删除</Button></>}><p>如需保留提示词，请先复制全文。</p></Modal>
    <Modal open={discardOpen} onClose={() => resolveDiscard(false)} title="当前文档保存失败" description="切换将丢失未保存的修改。你可以留在本文档重试保存。"
      footer={<><Button onClick={() => resolveDiscard(false)}>留在本文档</Button><Button variant="danger" onClick={() => resolveDiscard(true)}>放弃修改并切换</Button></>}><p>是否放弃当前修改？</p></Modal>
  </section>;
}
