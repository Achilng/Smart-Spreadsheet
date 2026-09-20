import type { ReactNode } from "react";
import { formatCount } from "../../../lib/utils/format";
export function ToolPage({ children, narrow = false }: { children: ReactNode; narrow?: boolean }) { return <div className={`rt-page${narrow ? " rt-narrow" : ""}`}>{children}</div>; }
export function ToolCard({ children, className = "" }: { children: ReactNode; className?: string }) { return <section className={`rt-card ${className}`}>{children}</section>; }
export function ToolError({ error }: { error?: string | null }) { return error ? <p className="rt-error" role="alert">{error}</p> : null; }
export function ToolProgress({ value, total, label }: { value: number; total: number; label?: string }) { return <div className="rt-progress" role="status"><span>{label} {formatCount(value)} / {formatCount(total)}</span><progress max={Math.max(1, total)} value={value} aria-label={label || "任务进度"} /></div>; }
export function ToolMetrics({ items }: { items: [number, string][] }) { return <div className="rt-metrics">{items.map(([value, label]) => <div key={label}><strong>{formatCount(value)}</strong><span>{label}</span></div>)}</div>; }
export const fileName = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;
