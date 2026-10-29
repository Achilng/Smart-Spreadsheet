import { useCallback, useSyncExternalStore } from "react";
import type { Editor } from "@tiptap/core";
import { Bold, ImagePlus, Italic, List, Redo2, Undo2 } from "lucide-react";
import { Button } from "../../ui/controls";

/** Transactions update only the controls whose formatting/undo state changed. */
export function PromptDocToolbar({ editor, disabled, onChooseImages }: {
  editor: Editor | null; disabled: boolean; onChooseImages: () => Promise<void>;
}) {
  const subscribe = useCallback((listener: () => void) => {
    if (!editor) return () => {};
    editor.on("transaction", listener);
    return () => { editor.off("transaction", listener); };
  }, [editor]);
  const snapshot = useCallback(() => {
    if (!editor || editor.isDestroyed) return 0;
    return Number(editor.isActive("paragraph")) | Number(editor.isActive("heading", { level: 2 })) << 1
      | Number(editor.isActive("bold")) << 2 | Number(editor.isActive("italic")) << 3
      | Number(editor.isActive("bulletList")) << 4 | Number(editor.can().undo()) << 5 | Number(editor.can().redo()) << 6;
  }, [editor]);
  const flags = useSyncExternalStore(subscribe, snapshot, () => 0);
  return <div className="r-pd-toolbar" role="toolbar" aria-label="提示词文档工具栏">
    <Button disabled={disabled} aria-pressed={Boolean(flags & 1)} onClick={() => editor?.chain().focus().setParagraph().run()}>正文</Button>
    <Button disabled={disabled} aria-pressed={Boolean(flags & 2)} onClick={() => editor?.chain().focus().toggleHeading({ level: 2 }).run()}>标题</Button>
    <Button disabled={disabled} aria-label="粗体" aria-pressed={Boolean(flags & 4)} onClick={() => editor?.chain().focus().toggleBold().run()}><Bold size={14} /></Button>
    <Button disabled={disabled} aria-label="斜体" aria-pressed={Boolean(flags & 8)} onClick={() => editor?.chain().focus().toggleItalic().run()}><Italic size={14} /></Button>
    <Button disabled={disabled} aria-pressed={Boolean(flags & 16)} onClick={() => editor?.chain().focus().toggleBulletList().run()}><List size={14} />列表</Button>
    <Button disabled={disabled} onClick={() => void onChooseImages()}><ImagePlus size={14} />插入图片</Button>
    <span className="r-pd-toolbar-separator" />
    <Button aria-label="撤销文档编辑" disabled={disabled || !(flags & 32)} onClick={() => editor?.chain().focus().undo().run()}><Undo2 size={14} /></Button>
    <Button aria-label="重做文档编辑" disabled={disabled || !(flags & 64)} onClick={() => editor?.chain().focus().redo().run()}><Redo2 size={14} /></Button>
  </div>;
}
