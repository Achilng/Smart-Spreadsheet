import { ImageOff } from "lucide-react";
import { thumbnails, useImage } from "./use-image";
import { detailPreviews } from "../../lib/images/progressive-images";

export function Thumbnail({ rowId, detail = false, alt, className }: { rowId?: number; detail?: boolean; alt: string; className?: string }) {
  const image = useImage(detail ? detailPreviews : thumbnails, rowId);
  return <div className={`r-image ${className ?? ""}`} data-loaded={Boolean(image.url)}>
    {image.url ? <img key={image.url} src={image.url} alt={alt} draggable={false} /> : image.error ? <span className="r-image-error"><ImageOff size={20} /><span>图片无法读取</span></span> : <span className="r-image-placeholder" aria-label="正在加载图片" />}
  </div>;
}
