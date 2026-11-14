import type { ComponentProps, ReactNode } from "react";
import { CheckCircleIcon, ChevronDownIcon, CircleSlashIcon, ClockIcon, WrenchIcon, XCircleIcon } from "lucide-react";
import { cn } from "../cn";
import { Badge, Collapsible, CollapsibleContent, CollapsibleTrigger } from "./primitives";

export type ToolState = "running" | "done" | "error" | "interrupted";

export type ToolProps = ComponentProps<typeof Collapsible>;

export const Tool = ({ className, ...props }: ToolProps) => (
  <Collapsible className={cn("group not-prose w-full rounded-md border", className)} {...props} />
);

const STATUS: Record<ToolState, { label: string; icon: ReactNode }> = {
  running: { label: "进行中", icon: <ClockIcon className="animate-pulse" /> },
  done: { label: "完成", icon: <CheckCircleIcon className="text-[var(--success)]" /> },
  error: { label: "失败", icon: <XCircleIcon className="text-[var(--danger)]" /> },
  interrupted: { label: "已中断", icon: <CircleSlashIcon className="text-[var(--warning)]" /> },
};

export type ToolHeaderProps = Omit<ComponentProps<typeof CollapsibleTrigger>, "title"> & { title: string; state: ToolState; summary?: string };

export const ToolHeader = ({ className, title, state, summary, ...props }: ToolHeaderProps) => (
  <CollapsibleTrigger className={cn("flex w-full items-center justify-between gap-4 px-3 py-2.5", className)} {...props}>
    <div className="flex min-w-0 items-center gap-2">
      <WrenchIcon className="size-4 shrink-0 text-muted-foreground" />
      <span className="shrink-0 font-medium text-sm">{title}</span>
      <Badge className="gap-1.5 text-xs" variant="secondary">{STATUS[state].icon}{STATUS[state].label}</Badge>
      {summary && <span className="truncate text-muted-foreground text-xs">{summary}</span>}
    </div>
    <ChevronDownIcon className="size-4 shrink-0 text-muted-foreground transition-transform group-data-[state=open]:rotate-180" />
  </CollapsibleTrigger>
);

export type ToolContentProps = ComponentProps<typeof CollapsibleContent>;

export const ToolContent = ({ className, ...props }: ToolContentProps) => (
  <CollapsibleContent
    className={cn(
      "border-t text-popover-foreground outline-none",
      "data-[state=closed]:fade-out-0 data-[state=closed]:slide-out-to-top-2 data-[state=open]:slide-in-from-top-2 data-[state=closed]:animate-out data-[state=open]:animate-in",
      className,
    )}
    {...props}
  />
);

const Code = ({ className, ...props }: ComponentProps<"pre">) => (
  <pre className={cn("max-h-64 select-text overflow-auto whitespace-pre-wrap break-words rounded-md p-3 font-mono text-xs leading-relaxed", className)} {...props} />
);

export type ToolInputProps = ComponentProps<"div"> & { input: string };

export const ToolInput = ({ className, input, ...props }: ToolInputProps) => (
  <div className={cn("space-y-2 overflow-hidden p-3", className)} {...props}>
    <h4 className="font-medium text-muted-foreground text-xs">参数</h4>
    <Code className="bg-muted/60">{input}</Code>
  </div>
);

export type ToolOutputProps = ComponentProps<"div"> & { output?: string | null; errorText?: string | null };

export const ToolOutput = ({ className, output, errorText, ...props }: ToolOutputProps) => {
  if (!(output || errorText)) return null;
  return (
    <div className={cn("space-y-2 overflow-hidden p-3 pt-0", className)} {...props}>
      <h4 className="font-medium text-muted-foreground text-xs">{errorText ? "出错了" : "结果"}</h4>
      <Code className={errorText ? "bg-destructive/10 text-destructive" : "bg-muted/60 text-foreground"}>{errorText || output}</Code>
    </div>
  );
};
