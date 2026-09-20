import type { AppSnapshot } from "../api/app";

/** Active library snapshot and query revision. */
export const libraryState = $state({
  snapshot: null as AppSnapshot | null,
  loaded: false,
  /** 资料库行集合变化（导入/删除）时 +1，数据视图据此整体重载 */
  dataVersion: 0,
  /** 本次行集合变化是否应保留视图滚动位置 */
  preserveScrollOnDataChange: false,
  /** 本次变化只改行内值（撤销/重做等），选区与缩略图缓存仍有效 */
  preserveSelectionOnDataChange: false
});
