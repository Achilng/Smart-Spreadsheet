import type { ViewMode } from "../utils/view-modes";

/** Window-local presentation preferences; no library operations. */
export const workspaceState = $state({
  viewMode: "gallery" as ViewMode,
  detailOpen: true,
  galleryCardSize: 190,
  tableRowHeight: 64,
  /** “更新现有图片”说明与来源选择弹窗 */
  updateImportOpen: false,
  /** 分组管理视图是否打开 */
  groupManageOpen: false,
  /** Discord 风格资料库筛选面板是否打开。 */
  filterOpen: false
});
