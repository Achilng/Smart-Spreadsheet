// AI Elements prompt input, reduced to what this app uses: text, a tools row and submit / stop.
import type { ComponentProps, FormEvent, HTMLAttributes, KeyboardEventHandler } from "react";
import { ArrowUpIcon, Loader2Icon, SquareIcon } from "lucide-react";
import { cn } from "../cn";
import { Button } from "./primitives";

export type PromptInputStatus = "ready" | "submitted" | "streaming";

export type PromptInputProps = Omit<HTMLAttributes<HTMLFormElement>, "onSubmit"> & { onSubmit: () => void };

export const PromptInput = ({ className, children, onSubmit, ...props }: PromptInputProps) => {
  const handleSubmit = (event: FormEvent<HTMLFormElement>) => { event.preventDefault(); onSubmit(); };
  return (
    <form className={cn("w-full", className)} onSubmit={handleSubmit} {...props}>
      <div className="flex w-full flex-col overflow-hidden rounded-2xl border border-input bg-background shadow-xs transition-[border-color,box-shadow] focus-within:border-ring/60 focus-within:ring-[3px] focus-within:ring-ring/15">
        {children}
      </div>
    </form>
  );
};

export type PromptInputTextareaProps = ComponentProps<"textarea">;

export const PromptInputTextarea = ({ className, onKeyDown, ...props }: PromptInputTextareaProps) => {
  const handleKeyDown: KeyboardEventHandler<HTMLTextAreaElement> = event => {
    onKeyDown?.(event);
    if (event.defaultPrevented || event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing) return;
    event.preventDefault();
    const form = event.currentTarget.form;
    const submit = form?.querySelector<HTMLButtonElement>('button[type="submit"]');
    if (!submit?.disabled) form?.requestSubmit();
  };
  return (
    <textarea
      className={cn("field-sizing-content max-h-48 min-h-16 w-full resize-none bg-transparent px-4 pt-3 pb-2 text-sm leading-relaxed outline-none placeholder:text-muted-foreground", className)}
      name="message"
      onKeyDown={handleKeyDown}
      {...props}
    />
  );
};

export type PromptInputFooterProps = HTMLAttributes<HTMLDivElement>;

export const PromptInputFooter = ({ className, ...props }: PromptInputFooterProps) => (
  <div className={cn("flex w-full items-center justify-between gap-1 px-3 pb-3", className)} {...props} />
);

export type PromptInputToolsProps = HTMLAttributes<HTMLDivElement>;

export const PromptInputTools = ({ className, ...props }: PromptInputToolsProps) => (
  <div className={cn("flex min-w-0 items-center gap-1", className)} {...props} />
);

export type PromptInputButtonProps = ComponentProps<typeof Button>;

export const PromptInputButton = ({ variant = "ghost", size = "sm", className, ...props }: PromptInputButtonProps) => (
  <Button className={cn("min-w-0", className)} size={size} variant={variant} {...props} />
);

export type PromptInputSubmitProps = ComponentProps<typeof Button> & { status?: PromptInputStatus; onStop?: () => void };

export const PromptInputSubmit = ({ className, variant = "default", size = "icon-sm", status = "ready", onStop, children, ...props }: PromptInputSubmitProps) => {
  const streaming = status === "streaming";
  const icon = status === "submitted" ? <Loader2Icon className="size-4 animate-spin" /> : streaming ? <SquareIcon className="size-3.5 fill-current" /> : <ArrowUpIcon className="size-4" />;
  return (
    <Button
      aria-label={streaming ? "停止" : "发送"}
      title={streaming ? "停止" : "发送"}
      className={cn("rounded-full", className)}
      size={size}
      variant={variant}
      {...props}
      type={streaming ? "button" : "submit"}
      disabled={streaming ? false : props.disabled}
      onClick={streaming ? onStop : props.onClick}
    >
      {children ?? icon}
    </Button>
  );
};
