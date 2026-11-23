import { useEffect, useState } from "react";
import { Dialog } from "radix-ui";
import { FileText, ImageOff, X } from "lucide-react";
import { agentAttachmentPreview, type AgentAttachment } from "../../../lib/api/agent";

const types: Record<string, string> = { png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", webp: "image/webp", pdf: "application/pdf" };
for (const ext of ["txt", "md", "csv", "tsv", "json", "log", "yaml", "yml", "xml"]) types[ext] = "text/plain";
export const attachmentAccept = Object.keys(types).map(ext => `.${ext}`).join(",");
const extension = (file: File) => /\.([a-z0-9]+)$/i.exec(file.name)?.[1]?.toLowerCase() ?? "";
const mime = (file: File) => Object.hasOwn(types, extension(file)) ? types[extension(file)] : "";

export function validateFiles(files: File[]): void {
  if (files.length > 4) throw new Error("一条消息最多添加 4 个附件");
  let images = 0, documents = 0;
  for (const f of files) {
    if (mime(f).startsWith("image/")) {
      if (f.size > 32 * 1024 ** 2) throw new Error(`原图「${f.name}」超过 32 MiB`);
      images += f.size;
    } else documents += f.size;
    if (!mime(f)) throw new Error(`暂不支持「${f.name}」。可添加 PNG、JPEG、WebP、PDF 或 UTF-8 文本文件。`);
    if (!f.size) throw new Error(`「${f.name}」是空文件`);
    if (mime(f) === "text/plain" && f.size > 256 * 1024) throw new Error(`文本「${f.name}」超过 256 KB，请截取需要的部分`);
  }
  if (images > 64 * 1024 ** 2) throw new Error("一条消息的图片合计不能超过 64 MiB");
  if (documents > 8 * 1024 ** 2) throw new Error("一条消息的 PDF 和文本合计不能超过 8 MiB（不含图片）");
}

export function encodeAttachment(file: File): Promise<AgentAttachment> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error(`无法读取「${file.name}」，请重新选择`));
    reader.onload = () => {
      const url = String(reader.result);
      resolve({ name: file.name, mime: mime(file), data: url.slice(url.indexOf(",") + 1) });
    };
    reader.readAsDataURL(file);
  });
}

export function DraftAttachment({ file, onRemove, disabled }: { file: File; onRemove: () => void; disabled: boolean }) {
  const [src, setSrc] = useState("");
  useEffect(() => {
    if (!mime(file).startsWith("image/")) return;
    const url = URL.createObjectURL(file);
    setSrc(url);
    return () => URL.revokeObjectURL(url);
  }, [file]);
  return <span className="ag-file-chip" title={file.name}>
    {src ? <img src={src} alt="" /> : <FileText className="icon sm" />}<span>{file.name}</span>
    <button type="button" onClick={onRemove} disabled={disabled} aria-label={`移除 ${file.name}`}><X className="icon sm" /></button>
  </span>;
}

export interface SentAttachment { name: string; image: boolean }
export function sentAttachments(value: unknown): SentAttachment[] {
  return Array.isArray(value) ? value.flatMap(a => typeof a?.name === "string"
    ? [{ name: a.name, image: typeof a.mime === "string" && a.mime.startsWith("image/") }] : []) : [];
}

const previews = new Map<string, Promise<string>>();
function loadPreview(conversationId: string, messageId: number, number: number, large: boolean): Promise<string> {
  const key = `${conversationId}:${messageId}:${number}:${large ? "l" : "s"}`;
  let pending = previews.get(key);
  if (!pending) {
    pending = agentAttachmentPreview(conversationId, messageId, number, large);
    pending.catch(() => previews.delete(key));
    previews.set(key, pending);
    if (previews.size > 200) previews.delete(previews.keys().next().value!);
  }
  return pending;
}

function usePreview(conversationId: string, messageId: number, number: number, large: boolean, enabled = true) {
  const [state, setState] = useState<{ key: string; url: string | null; failed: boolean }>({ key: "", url: null, failed: false });
  const key = `${conversationId}:${messageId}:${number}:${large}`;
  useEffect(() => {
    if (!enabled) return;
    let alive = true;
    loadPreview(conversationId, messageId, number, large)
      .then(url => { if (alive) setState({ key, url, failed: false }); })
      .catch(() => { if (alive) setState({ key, url: null, failed: true }); });
    return () => { alive = false; };
  }, [conversationId, messageId, number, large, enabled, key]);
  return state.key === key ? state : { url: null, failed: false };
}

/** An image the user sent in a message, shown as a thumbnail that opens a larger view. */
export function SentImage({ conversationId, messageId, number, name }: { conversationId: string; messageId: number; number: number; name: string }) {
  const { url, failed } = usePreview(conversationId, messageId, number, false);
  const [open, setOpen] = useState(false);
  return <>
    <button type="button" className={`ag-thumb ag-sent-image${url ? "" : " is-empty"}`} title={name} aria-label={`查看图片 ${name}`} onClick={() => setOpen(true)}>
      {url ? <img src={url} alt="" draggable={false} /> : failed ? <ImageOff className="icon sm" aria-hidden="true" /> : null}
    </button>
    <Dialog.Root open={open} onOpenChange={setOpen}><Dialog.Portal>
      <Dialog.Overlay className="ag-viewer-overlay" />
      <Dialog.Content className="ag-viewer" aria-describedby={undefined}>
        <header className="ag-viewer-head">
          <Dialog.Title className="ag-viewer-title">{name}</Dialog.Title>
          <Dialog.Close asChild><button type="button" className="ag-viewer-btn" aria-label="关闭"><X className="icon" /></button></Dialog.Close>
        </header>
        <div className="ag-viewer-stage">{open && <LargeSentImage conversationId={conversationId} messageId={messageId} number={number} small={url} name={name} />}</div>
      </Dialog.Content>
    </Dialog.Portal></Dialog.Root>
  </>;
}

function LargeSentImage({ conversationId, messageId, number, small, name }: { conversationId: string; messageId: number; number: number; small: string | null; name: string }) {
  const large = usePreview(conversationId, messageId, number, true);
  const url = large.url ?? small;
  if (!url) return <div className="ag-viewer-missing">{large.failed ? <><ImageOff className="icon" />图片读取不到</> : <span className="spinner" aria-label="加载中" />}</div>;
  return <img src={url} alt={name} draggable={false} />;
}
