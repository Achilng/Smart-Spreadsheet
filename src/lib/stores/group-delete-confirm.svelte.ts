import type { GroupSummary } from "../api";

/**
 * 删除分组的统一确认状态。删除入口有两个（管理分组对话框、分区右键菜单），
 * 共用同一个确认对话框，文案统一说明“图片不会被删除”。
 */
export const groupDeleteConfirm = $state({
  open: false,
  group: null as GroupSummary | null,
  busy: false,
  /** 确认后由发起方执行的实际删除动作 */
  action: null as (() => Promise<void>) | null,
});

export function requestGroupDelete(
  group: GroupSummary,
  action: () => Promise<void>,
): void {
  groupDeleteConfirm.group = group;
  groupDeleteConfirm.action = action;
  groupDeleteConfirm.busy = false;
  groupDeleteConfirm.open = true;
}

export function cancelGroupDelete(): void {
  if (groupDeleteConfirm.busy) return;
  groupDeleteConfirm.open = false;
  groupDeleteConfirm.group = null;
  groupDeleteConfirm.action = null;
}

export async function confirmGroupDelete(): Promise<void> {
  const action = groupDeleteConfirm.action;
  if (!action || groupDeleteConfirm.busy) return;
  groupDeleteConfirm.busy = true;
  try {
    await action();
  } finally {
    groupDeleteConfirm.busy = false;
    groupDeleteConfirm.open = false;
    groupDeleteConfirm.group = null;
    groupDeleteConfirm.action = null;
  }
}
