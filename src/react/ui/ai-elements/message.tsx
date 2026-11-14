import type { ComponentProps, HTMLAttributes } from "react";
import { memo } from "react";
import { cjk } from "@streamdown/cjk";
import { Streamdown, type StreamdownProps } from "streamdown";
import { cn } from "../cn";
import { Hint } from "../controls";
import { Button } from "./primitives";

export type MessageProps = HTMLAttributes<HTMLDivElement> & { from: "user" | "assistant" };

export const Message = ({ className, from, ...props }: MessageProps) => (
  <div className={cn("group flex w-full max-w-[95%] flex-col gap-2", from === "user" ? "is-user ml-auto justify-end" : "is-assistant", className)} {...props} />
);

export type MessageContentProps = HTMLAttributes<HTMLDivElement>;

export const MessageContent = ({ children, className, ...props }: MessageContentProps) => (
  <div
    className={cn(
      "flex w-fit min-w-0 max-w-full flex-col gap-2 overflow-hidden text-sm",
      "group-[.is-user]:ml-auto group-[.is-user]:rounded-lg group-[.is-user]:bg-secondary group-[.is-user]:px-4 group-[.is-user]:py-3 group-[.is-user]:text-foreground",
      "group-[.is-assistant]:text-foreground",
      className,
    )}
    {...props}
  >
    {children}
  </div>
);

export type MessageActionsProps = ComponentProps<"div">;

export const MessageActions = ({ className, children, ...props }: MessageActionsProps) => (
  <div className={cn("flex items-center gap-1", className)} {...props}>{children}</div>
);

export type MessageActionProps = ComponentProps<typeof Button> & { tooltip?: string; label?: string };

export const MessageAction = ({ tooltip, children, label, variant = "ghost", size = "icon-sm", ...props }: MessageActionProps) => {
  const button = <Button size={size} variant={variant} aria-label={label || tooltip} {...props}>{children}</Button>;
  return tooltip ? <Hint text={tooltip}>{button}</Hint> : button;
};

const plugins: StreamdownProps["plugins"] = { cjk };
const controls: StreamdownProps["controls"] = { table: { copy: true, download: false, fullscreen: false }, code: { copy: true, download: false } };
const translations: StreamdownProps["translations"] = { copyCode: "复制代码", copied: "已复制", copyTable: "复制表格", copyTableAsCsv: "复制为 CSV", copyTableAsMarkdown: "复制为 Markdown", copyTableAsTsv: "复制为 TSV", close: "关闭" };
// The window has no browser chrome to navigate with, so links render as plain text.
const components: StreamdownProps["components"] = {
  a: ({ children, href }) => <span className="underline underline-offset-2" title={typeof href === "string" ? href : undefined}>{children}</span>,
};

export type MessageResponseProps = ComponentProps<typeof Streamdown>;

export const MessageResponse = memo(
  ({ className, ...props }: MessageResponseProps) => (
    <Streamdown
      className={cn("size-full [&>*:first-child]:mt-0 [&>*:last-child]:mb-0", className)}
      plugins={plugins}
      controls={controls}
      translations={translations}
      components={components}
      {...props}
    />
  ),
  (prevProps, nextProps) => prevProps.children === nextProps.children && prevProps.isAnimating === nextProps.isAnimating,
);

MessageResponse.displayName = "MessageResponse";
