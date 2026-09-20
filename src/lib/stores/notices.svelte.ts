export interface Notice {
  tone: "error" | "success";
  text: string;
}

export interface QueuedNotice extends Notice {
  id: number;
}

export const noticeState = $state({
  /** 通知队列：最多同时展示 3 条；error 不自动消失，success 5 秒后过期 */
  notices: [] as QueuedNotice[]
});

let noticeSerial = 1;

const noticeTimers = new Map<number, number>();

/**
 * 入队一条通知（null 表示清空全部）。
 * 通知按队列堆叠而不是互相覆盖；success 5 秒后自动过期，error 常驻到手动关闭。
 */
export function setNotice(notice: Notice | null): void {
  if (notice === null) {
    for (const timer of noticeTimers.values()) {
      window.clearTimeout(timer);
    }
    noticeTimers.clear();
    noticeState.notices = [];
    return;
  }
  const queued: QueuedNotice = { ...notice, id: noticeSerial };
  noticeSerial += 1;
  // 最多同时保留 3 条，挤掉最旧的一条
  const next = [...noticeState.notices, queued];
  while (next.length > 3) {
    const removed = next.shift();
    if (removed) {
      const timer = noticeTimers.get(removed.id);
      if (timer !== undefined) {
        window.clearTimeout(timer);
        noticeTimers.delete(removed.id);
      }
    }
  }
  noticeState.notices = next;
  if (queued.tone === "success") {
    const timer = window.setTimeout(() => {
      dismissNotice(queued.id);
    }, 5000);
    noticeTimers.set(queued.id, timer);
  }
}

export function dismissNotice(id: number): void {
  const timer = noticeTimers.get(id);
  if (timer !== undefined) {
    window.clearTimeout(timer);
    noticeTimers.delete(id);
  }
  noticeState.notices = noticeState.notices.filter(notice => notice.id !== id);
}
