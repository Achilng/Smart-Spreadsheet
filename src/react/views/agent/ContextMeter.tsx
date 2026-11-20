import { Tooltip } from "../../ui/controls";
import type { AgentContextUsage } from "../../../lib/api/agent";

const RADIUS = 7.5;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

function tokens(value: number): string {
  if (value < 1000) return String(Math.round(value));
  const k = value / 1000;
  return `${k >= 100 ? Math.round(k) : Number(k.toFixed(1))}k`;
}

/** Ring showing how much of the model's context is left ("fuel"); details on hover. */
export function ContextMeter({ usage, window }: { usage: AgentContextUsage; window: number }) {
  const used = Math.max(0, usage.used);
  const usedShare = Math.min(1, used / Math.max(1, window));
  const usedPercent = Math.round(usedShare * 100);
  const left = 1 - usedShare;
  const level = left < 0.1 ? "is-low" : left < 0.25 ? "is-warn" : "";
  return <Tooltip.Root>
    <Tooltip.Trigger asChild>
      <button type="button" className={`context-meter ${level}`} aria-label={`上下文已用 ${usedPercent}%，剩余 ${100 - usedPercent}%`}>
        <svg viewBox="0 0 20 20" aria-hidden="true">
          <circle className="context-meter__track" cx="10" cy="10" r={RADIUS} />
          <circle className="context-meter__fuel" cx="10" cy="10" r={RADIUS}
            strokeDasharray={`${CIRCUMFERENCE * left} ${CIRCUMFERENCE}`} transform="rotate(-90 10 10)" />
        </svg>
      </button>
    </Tooltip.Trigger>
    <Tooltip.Portal>
      <Tooltip.Content sideOffset={6} className="r-tooltip context-meter__tip" collisionPadding={8}>
        <strong>上下文窗口</strong>
        <span>{usedPercent}% 已用（剩余 {100 - usedPercent}%）</span>
        <span>已用 {tokens(used)} tokens，共 {tokens(window)}</span>
        {usage.source === "estimate" && <span className="context-meter__note">站点没有返回用量，这是估算值</span>}
        {!!usage.compactions && <span className="context-meter__note">较早的对话已自动整理成摘要 {usage.compactions} 次</span>}
        {!usage.compactions && usedShare >= 0.7 && <span className="context-meter__note">快满时会自动把较早的对话整理成摘要</span>}
      </Tooltip.Content>
    </Tooltip.Portal>
  </Tooltip.Root>;
}
