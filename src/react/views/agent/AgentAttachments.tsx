import { useEffect, useState } from "react";
import { FileText, X } from "lucide-react";
import type { AgentAttachment } from "../../../lib/api/agent";

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

export function attachmentNames(value: unknown): string[] {
  return Array.isArray(value) ? value.flatMap(a => typeof a?.name === "string" ? [a.name] : []) : [];
}
