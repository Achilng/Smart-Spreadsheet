import { Check, CircleAlert, X } from "lucide-react";
import { dismissNotice, useNotices } from "../state/notices";
import { Button } from "./controls";

export function Notices() {
  const notices = useNotices(state => state.notices);
  return <div className="r-notices">{notices.map(notice => <div key={notice.id} className={`r-notice ${notice.tone}`} role={notice.tone === "error" ? "alert" : "status"}>
    {notice.tone === "error" ? <CircleAlert size={17} /> : <Check size={17} />}<span>{notice.text}</span>
    <Button variant="ghost" size="icon" aria-label="关闭通知" onClick={() => dismissNotice(notice.id)}><X size={14} /></Button>
  </div>)}</div>;
}
