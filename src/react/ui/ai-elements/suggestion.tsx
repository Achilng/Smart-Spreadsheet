import type { ComponentProps } from "react";
import { cn } from "../cn";
import { Button } from "./primitives";

export type SuggestionsProps = ComponentProps<"div">;

export const Suggestions = ({ className, children, ...props }: SuggestionsProps) => (
  <div className={cn("flex flex-wrap items-center justify-center gap-2", className)} {...props}>{children}</div>
);

export type SuggestionProps = Omit<ComponentProps<typeof Button>, "onClick"> & { suggestion: string; onClick?: (suggestion: string) => void };

export const Suggestion = ({ suggestion, onClick, className, variant = "outline", size = "sm", children, ...props }: SuggestionProps) => (
  <Button className={cn("h-auto min-h-8 max-w-full whitespace-normal rounded-full px-4 py-1.5 text-left font-normal", className)} onClick={() => onClick?.(suggestion)} size={size} variant={variant} {...props}>
    {children || suggestion}
  </Button>
);
