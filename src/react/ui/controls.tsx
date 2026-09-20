import { useId, useLayoutEffect, useRef, type ComponentProps, type ReactNode } from "react";
import { Checkbox as CheckboxPrimitive, Dialog, DropdownMenu, Slider as SliderPrimitive, Slot, Tooltip } from "radix-ui";
import { Check, ChevronDown, X } from "lucide-react";
import { cn } from "./cn";

// Project controls follow shadcn's composition model: Radix owns behavior,
// while this source-owned layer keeps the application's existing visual language.
export function Button({ className, variant = "default", size = "default", asChild = false, ...props }: ComponentProps<"button"> & {
  variant?: "default" | "primary" | "ghost" | "danger";
  size?: "default" | "sm" | "icon";
  asChild?: boolean;
}) {
  const Component = asChild ? Slot.Root : "button";
  return <Component type="button" className={cn("btn", variant !== "default" && `btn-${variant}`, size !== "default" && `btn-${size}`, className)} {...props} />;
}

export function Input({ className, ...props }: ComponentProps<"input">) {
  return <input className={cn("r-input", className)} {...props} />;
}

export function Textarea({ className, ...props }: ComponentProps<"textarea">) {
  return <textarea className={cn("r-input r-textarea", className)} {...props} />;
}

export function Checkbox({ className, ...props }: ComponentProps<typeof CheckboxPrimitive.Root>) {
  return <CheckboxPrimitive.Root className={cn("r-checkbox", className)} {...props}>
    <CheckboxPrimitive.Indicator className="r-checkbox-indicator"><Check size={12} strokeWidth={2.4} /></CheckboxPrimitive.Indicator>
  </CheckboxPrimitive.Root>;
}

export function Slider({ className, ...props }: ComponentProps<typeof SliderPrimitive.Root>) {
  return <SliderPrimitive.Root className={cn("r-slider", className)} {...props}>
    <SliderPrimitive.Track className="r-slider-track"><SliderPrimitive.Range className="r-slider-range" /></SliderPrimitive.Track>
    <SliderPrimitive.Thumb className="r-slider-thumb" aria-label={props["aria-label"]} />
  </SliderPrimitive.Root>;
}

export function Hint({ children, text }: { children: ReactNode; text: string }) {
  return <Tooltip.Root><Tooltip.Trigger asChild>{children}</Tooltip.Trigger><Tooltip.Portal>
    <Tooltip.Content sideOffset={6} className="r-tooltip" collisionPadding={8}>{text}</Tooltip.Content>
  </Tooltip.Portal></Tooltip.Root>;
}

export interface MenuItem {
  label: string;
  hint?: string;
  checked?: boolean;
  danger?: boolean;
  disabled?: boolean;
  separator?: boolean;
  keepOpen?: boolean;
  action: () => void;
}

export function Menu({ label, items, variant = "ghost", disabled, direction = "down", heading, className }: {
  label: ReactNode;
  items: MenuItem[];
  variant?: "primary" | "ghost" | "default";
  disabled?: boolean;
  direction?: "down" | "up";
  heading?: string;
  className?: string;
}) {
  return <DropdownMenu.Root><DropdownMenu.Trigger asChild>
    <Button variant={variant} className={cn("r-menu-trigger", className)} disabled={disabled}>{label}<ChevronDown size={13} className="r-caret" /></Button>
  </DropdownMenu.Trigger><DropdownMenu.Portal>
    <DropdownMenu.Content side={direction === "up" ? "top" : "bottom"} align="end" sideOffset={6} collisionPadding={8} className="r-menu">
      {heading && <DropdownMenu.Label className="r-menu-heading">{heading}</DropdownMenu.Label>}
      {items.map(item => <MenuEntry key={item.label} item={item} />)}
    </DropdownMenu.Content>
  </DropdownMenu.Portal></DropdownMenu.Root>;
}

function MenuEntry({ item }: { item: MenuItem }) {
  const content = <><span className="r-menu-copy"><span>{item.label}</span>{item.hint && <small>{item.hint}</small>}</span>{item.checked && <Check size={14} className="r-menu-check" />}</>;
  const props = {
    disabled: item.disabled,
    className: cn("r-menu-item", item.danger && "is-danger"),
    onSelect: (event: Event) => { if (item.keepOpen) event.preventDefault(); item.action(); },
  };
  return <>{item.separator && <DropdownMenu.Separator className="r-menu-separator" />}
    {item.checked === undefined ? <DropdownMenu.Item {...props}>{content}</DropdownMenu.Item>
      : <DropdownMenu.CheckboxItem checked={item.checked} {...props}>{content}</DropdownMenu.CheckboxItem>}
  </>;
}

export function Modal({ open, onClose, title, description, busy = false, width = 440, children, footer }: {
  open: boolean;
  onClose: () => void;
  title: string;
  description?: string;
  busy?: boolean;
  width?: number;
  children: ReactNode;
  footer?: ReactNode;
}) {
  const descriptionId = useId();
  const opener = useRef<HTMLElement | null>(null);
  const content = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => { if (open) opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null; }, [open]);
  return <Dialog.Root open={open} onOpenChange={next => { if (!next && !busy) onClose(); }}><Dialog.Portal>
    <Dialog.Overlay className="r-dialog-overlay" />
    <Dialog.Content ref={content} className="r-dialog" style={{ width }} aria-describedby={description ? descriptionId : undefined}
      onOpenAutoFocus={event => {
        const firstInput = content.current?.querySelector<HTMLElement>('input:not([disabled]), textarea:not([disabled]), [data-autofocus]');
        if (firstInput) { event.preventDefault(); firstInput.focus(); }
      }}
      onCloseAutoFocus={event => { event.preventDefault(); opener.current?.focus(); }}
      onEscapeKeyDown={event => { if (busy) event.preventDefault(); }}
      onPointerDownOutside={event => { if (busy) event.preventDefault(); }}>
      <header className="r-dialog-header"><div><Dialog.Title className="r-dialog-title">{title}</Dialog.Title>
        {description && <Dialog.Description id={descriptionId} className="r-dialog-description">{description}</Dialog.Description>}</div>
        <Dialog.Close asChild><Button variant="ghost" size="icon" disabled={busy} aria-label="关闭对话框"><X size={16} /></Button></Dialog.Close>
      </header>
      <div className="r-dialog-body">{children}</div>
      {footer && <footer className="r-dialog-footer">{footer}</footer>}
    </Dialog.Content>
  </Dialog.Portal></Dialog.Root>;
}

export { Tooltip };
