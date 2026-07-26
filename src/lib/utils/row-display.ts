import type { RowRecord } from "../api";

/** 从图片路径提取文件名（GalleryCard 图注 / 详情面板标题共用）。 */
export function rowFileName(row: RowRecord): string | null {
  const path = row.imagePath ?? row.storedImagePath;
  return path?.split(/[\\/]/).pop() ?? null;
}

/** "1216 × 832" 形式的分辨率文本；宽高缺失（旧数据）时返回 null。 */
export function rowResolution(row: RowRecord): string | null {
  return row.imageWidth && row.imageHeight
    ? `${row.imageWidth} × ${row.imageHeight}`
    : null;
}
