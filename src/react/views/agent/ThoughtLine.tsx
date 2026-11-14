// React port of H3 Studio's Thought Line, itself adapted from React Bits.
// Copyright (c) 2026 David Haz — MIT + Commons Clause; see ./react-bits-LICENSE.md.
import { useEffect, useState, type ReactNode } from "react";
import { ChevronDown } from "lucide-react";

export interface ThoughtStep { key: string; state: "done" | "active" | "failed" | "note"; content: ReactNode }

function clock(ms: number): string {
  const seconds = Math.max(0, ms / 1000);
  return seconds < 60 ? `${seconds.toFixed(1)} 秒` : `${Math.floor(seconds / 60)} 分 ${(seconds % 60).toFixed(1)} 秒`;
}

export function ThoughtLine({ working, startedAt, endedAt, steps, doneLabel = "已完成" }: {
  working: boolean;
  startedAt: number | null;
  /** When the work ended, for a line that was already finished when it appeared. */
  endedAt?: number | null;
  steps: ThoughtStep[];
  doneLabel?: string;
}) {
  const [open, setOpen] = useState(working);
  const [now, setNow] = useState<number | null>(null);
  useEffect(() => {
    setOpen(working);
    if (!working) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 100);
    return () => { clearInterval(timer); setNow(Date.now()); };
  }, [working]);

  // A line watched to its end keeps the time it stopped at instead of jumping to the stored one.
  const end = now ?? endedAt ?? null;
  const elapsed = startedAt !== null && end !== null ? end - startedAt : null;
  const label = working ? "思考中" : doneLabel;
  const expanded = open && steps.length > 0;
  return <div className="thought-line" data-working={working ? "" : undefined} data-open={expanded ? "" : undefined}>
    <button type="button" className="thought-line__head" aria-expanded={steps.length ? open : undefined} aria-disabled={!steps.length} onClick={() => { if (steps.length) setOpen(!open); }}>
      <span className="thought-line__glyph" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinejoin="round"><path d="M10 3 12.3 9.7 19 12l-6.7 2.3L10 21l-2.3-6.7L1 12l6.7-2.3Z M20 2l.8 2.2L23 5l-2.2.8L20 8l-.8-2.2L17 5l2.2-.8Z" /></svg></span>
      <span className="thought-line__label"><span key={label} className="thought-line__breath">{label}</span></span>
      {elapsed !== null && (working || elapsed >= 100) && <span className="thought-line__timer" aria-hidden="true">{clock(elapsed)}</span>}
      {steps.length > 0 && <ChevronDown className="thought-line__chevron icon sm" aria-hidden="true" />}
    </button>
    <span className="sr-only" role="status">{label}</span>
    <div className="thought-line__trace" data-open={expanded ? "" : undefined} aria-hidden={!expanded} inert={!expanded}>
      <div className="thought-line__fold"><div className="thought-line__steps">
        {steps.map(step => <div key={step.key} className="thought-line__step" data-state={step.state}>
          <span className="thought-line__mark" aria-hidden="true">
            {step.state === "done" ? <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6"><path d="m3 8 3 3 7-7" /></svg>
              : step.state === "failed" ? <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6"><path d="m4 4 8 8M12 4l-8 8" /></svg>
                : step.state === "active" ? <span className="thought-line__pulse" />
                  : <span className="thought-line__dot" />}
          </span>
          <div className="thought-line__body">{step.content}</div>
        </div>)}
      </div></div>
    </div>
  </div>;
}
