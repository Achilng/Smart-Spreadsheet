import type { ComponentProps, ReactNode } from "react";
import { createContext, memo, useContext, useEffect, useRef, useState } from "react";
import { BrainIcon, ChevronDownIcon } from "lucide-react";
import { cn } from "../cn";
import { MessageResponse } from "./message";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "./primitives";
import { Shimmer } from "./shimmer";

type ReasoningContextValue = { isStreaming: boolean; isOpen: boolean; setIsOpen: (open: boolean) => void; duration: number | undefined };

const ReasoningContext = createContext<ReasoningContextValue | null>(null);

export const useReasoning = () => {
  const context = useContext(ReasoningContext);
  if (!context) throw new Error("Reasoning components must be used within Reasoning");
  return context;
};

export type ReasoningProps = Omit<ComponentProps<typeof Collapsible>, "open" | "onOpenChange"> & { isStreaming?: boolean; defaultOpen?: boolean; duration?: number };

const AUTO_CLOSE_DELAY = 1000;

export const Reasoning = memo(({ className, isStreaming = false, defaultOpen = true, duration: durationProp, children, ...props }: ReasoningProps) => {
  const [isOpen, setIsOpen] = useState(defaultOpen);
  const [measured, setMeasured] = useState<number | undefined>(undefined);
  const [hasAutoClosed, setHasAutoClosed] = useState(false);
  const startTime = useRef<number | null>(null);

  useEffect(() => {
    if (isStreaming) { startTime.current ??= Date.now(); }
    else if (startTime.current !== null) { setMeasured(Math.ceil((Date.now() - startTime.current) / 1000)); startTime.current = null; }
  }, [isStreaming]);

  // Stays open while streaming, then folds itself away once.
  useEffect(() => {
    if (defaultOpen && !isStreaming && isOpen && !hasAutoClosed) {
      const timer = setTimeout(() => { setIsOpen(false); setHasAutoClosed(true); }, AUTO_CLOSE_DELAY);
      return () => clearTimeout(timer);
    }
  }, [isStreaming, isOpen, defaultOpen, hasAutoClosed]);

  return (
    <ReasoningContext.Provider value={{ isStreaming, isOpen, setIsOpen, duration: durationProp ?? measured }}>
      <Collapsible className={cn("not-prose", className)} onOpenChange={setIsOpen} open={isOpen} {...props}>{children}</Collapsible>
    </ReasoningContext.Provider>
  );
});

export type ReasoningTriggerProps = ComponentProps<typeof CollapsibleTrigger> & { getThinkingMessage?: (isStreaming: boolean, duration?: number) => ReactNode };

const defaultGetThinkingMessage = (isStreaming: boolean, duration?: number) => {
  if (isStreaming || duration === 0) return <Shimmer duration={1}>思考中…</Shimmer>;
  if (duration === undefined) return <p>思考过程</p>;
  return <p>思考了 {duration} 秒</p>;
};

export const ReasoningTrigger = memo(({ className, children, getThinkingMessage = defaultGetThinkingMessage, ...props }: ReasoningTriggerProps) => {
  const { isStreaming, isOpen, duration } = useReasoning();
  return (
    <CollapsibleTrigger className={cn("flex w-full items-center gap-2 text-muted-foreground text-sm transition-colors hover:text-foreground", className)} {...props}>
      {children ?? <>
        <BrainIcon className="size-4" />
        {getThinkingMessage(isStreaming, duration)}
        <ChevronDownIcon className={cn("size-4 transition-transform", isOpen ? "rotate-180" : "rotate-0")} />
      </>}
    </CollapsibleTrigger>
  );
});

export type ReasoningContentProps = Omit<ComponentProps<typeof CollapsibleContent>, "children"> & { children: string };

export const ReasoningContent = memo(({ className, children, ...props }: ReasoningContentProps) => (
  <CollapsibleContent
    className={cn(
      "mt-3 border-l-2 pl-4 text-muted-foreground text-sm outline-none",
      "data-[state=closed]:fade-out-0 data-[state=closed]:slide-out-to-top-2 data-[state=open]:slide-in-from-top-2 data-[state=closed]:animate-out data-[state=open]:animate-in",
      className,
    )}
    {...props}
  >
    <MessageResponse>{children}</MessageResponse>
  </CollapsibleContent>
));

Reasoning.displayName = "Reasoning";
ReasoningTrigger.displayName = "ReasoningTrigger";
ReasoningContent.displayName = "ReasoningContent";
