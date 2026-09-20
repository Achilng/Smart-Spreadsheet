import { create } from "zustand";

interface Notice { id: number; tone: "error" | "success"; text: string }
export const useNotices = create<{ notices: Notice[] }>(() => ({ notices: [] }));
const timers = new Map<number, ReturnType<typeof setTimeout>>();
let serial = 0;

export function dismissNotice(id: number): void {
  clearTimeout(timers.get(id));
  timers.delete(id);
  useNotices.setState(state => ({ notices: state.notices.filter(notice => notice.id !== id) }));
}

export function notify(text: string, tone: Notice["tone"] = "success"): void {
  const notice = { id: ++serial, tone, text };
  const notices = [...useNotices.getState().notices, notice];
  while (notices.length > 3) dismissNotice(notices.shift()!.id);
  useNotices.setState({ notices });
  if (tone === "success") timers.set(notice.id, setTimeout(() => dismissNotice(notice.id), 5000));
}
