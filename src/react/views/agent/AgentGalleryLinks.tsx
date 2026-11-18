import type { Link, Root, Parents } from "mdast";
import { RowThumb, useImageViewer } from "./AgentMedia";

const prefix = "#gallery-rows=";
const rowUrl = /^row:([1-9]\d*)$/;

/** Group actual Markdown images (not examples in code) before URL sanitization.
 * Use an inert fragment, never a filesystem path or a custom protocol in the DOM. */
export function remarkGalleryImages() {
  return (root: Root) => {
    function walk(parent: Parents) {
      const nodes = parent.children;
      for (let i = 0; i < nodes.length; i++) {
        const node = nodes[i];
        if (node.type !== "image") {
          if ("children" in node && node.type !== "link" && node.type !== "linkReference") walk(node);
          continue;
        }
        const images = new Map<number, string>();
        let end = i;
        for (let j = i; j < nodes.length; j++) {
          const next = nodes[j];
          if (next.type === "break" || (next.type === "text" && /^\s*$/.test(next.value))) continue;
          if (next.type !== "image") break;
          const id = Number(rowUrl.exec(next.url)?.[1]);
          if (!Number.isSafeInteger(id)) break;
          images.set(id, (next.alt ?? "").replace(/\s+/g, " ").trim());
          end = j;
        }
        if (!images.size) continue;
        const link: Link = { type: "link", url: prefix + [...images.keys()].join(","), title: [...images.values()].join("\n"), children: [{ type: "text", value: "图库图片" }] };
        nodes.splice(i, end - i + 1, link);
      }
    }
    walk(root);
  };
}

export function GalleryLink({ href, title }: { href: string; title?: string }) {
  const view = useImageViewer();
  const source = href.slice(prefix.length);
  if (!/^[1-9]\d*(?:,[1-9]\d*)*$/.test(source)) return null;
  const ids = source.split(",").map(Number);
  if (!ids.every(Number.isSafeInteger)) return null;
  const labels = title?.split("\n");
  const size = ids.length === 1 ? 160 : 96;
  const open = (index: number) => view({ title: "回复中的图片", ids, index });
  return <span className="ag-chat-images" role="group" aria-label={`回复中的 ${ids.length} 张图片`}>
    {ids.slice(0, 12).map((id, index) => <RowThumb key={`${index}-${id}`} id={id} size={size} label={labels?.[index] || undefined} onOpen={() => open(index)} />)}
    {ids.length > 12 && <button type="button" className="ag-thumb ag-thumb-more" style={{ width: size, height: size }} onClick={() => open(12)} aria-label={`查看其余 ${ids.length - 12} 张图`}>+{ids.length - 12}</button>}
  </span>;
}

export const isGalleryLink = (href?: string) => href?.startsWith(prefix);
