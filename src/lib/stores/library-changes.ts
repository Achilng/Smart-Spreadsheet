import { libraryState } from "./library-state.svelte";
import { notifyToolboxLibraryChanged } from "../windows/library-events";

/**
 * 通知数据视图重载行集合。删除只是就地移除行，可保留滚动位置；
 * 导入、换库等数据集整体变化继续回顶。
 * preserveSelection：仅值变化（撤销/重做、批量编辑）时为 true——行集合没变，
 * 选区与缩略图缓存仍然有效，不应被清空（否则每次 Ctrl+Z 都会丢选区、闪图）。
 */
export function bumpDataVersion(
  options: { preserveScroll?: boolean; preserveSelection?: boolean; origin?: "main" | "toolbox" } = {},
): void {
  libraryState.preserveScrollOnDataChange = options.preserveScroll ?? false;
  libraryState.preserveSelectionOnDataChange = options.preserveSelection ?? false;
  libraryState.dataVersion += 1;
  // 同步告知工具箱窗口资料库已变化（未打开时静默失败）。
  // origin 让工具箱区分“主窗口自己的编辑”（需失效工具箱撤销栈）与
  // “工具箱操作回流的通知”（不能反过来清掉工具箱刚记下的撤销）。
  notifyToolboxLibraryChanged(options.origin ?? "main");
}
