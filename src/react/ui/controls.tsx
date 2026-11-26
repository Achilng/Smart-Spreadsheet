import { Fragment, useId, useLayoutEffect, useRef, useState, type ComponentProps, type ReactElement, type ReactNode } from "react";
import { Checkbox as CheckboxPrimitive, ContextMenu, Dialog, DropdownMenu, Select as SelectPrimitive, Slider as SliderPrimitive, Slot, Tooltip } from "radix-ui";
import { Check, ChevronDown, Minus, Search, X } from "lucide-react";
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

export function SearchField({ className, value, onClear, clearLabel = "清除搜索", ...props }: ComponentProps<"input"> & { onClear?: () => void; clearLabel?: string }) {
  return <div className={cn("r-search-field", className)}>
    <Search size={15} aria-hidden="true" />
    <input type="text" className="r-input is-pill" value={value} {...props} />
    {value && onClear && <Button variant="ghost" size="icon" aria-label={clearLabel} onClick={onClear}><X size={13} /></Button>}
  </div>;
}

export function Segmented<T extends string>({ value, onChange, options, label }: { value: T; onChange: (value: T) => void; options: readonly { value: T; label: string; icon?: ReactNode }[]; label: string }) {
  return <div className="r-segmented" role="group" aria-label={label}>
    {options.map(option => <button type="button" key={option.value} aria-pressed={value === option.value} onClick={() => onChange(option.value)}>{option.icon}{option.label}</button>)}
  </div>;
}

export function Checkbox({ className, ...props }: ComponentProps<typeof CheckboxPrimitive.Root>) {
  return <CheckboxPrimitive.Root className={cn("r-checkbox", className)} {...props}>
    <CheckboxPrimitive.Indicator className="r-checkbox-indicator">{props.checked === "indeterminate" ? <Minus size={12} strokeWidth={2.4} /> : <Check size={12} strokeWidth={2.4} />}</CheckboxPrimitive.Indicator>
  </CheckboxPrimitive.Root>;
}

/** [value, label] or [value, label, second-line hint]. */
export type SelectOption<T extends string> = readonly [T, string] | readonly [T, string, string];

export function Select<T extends string>({ value, onChange, options, label, disabled, pill = false, appearance = "field", className, heading, placeholder, align = "start", autoFocus }: {
  value: T;
  onChange: (value: T) => void;
  options: readonly SelectOption<T>[];
  label: string;
  disabled?: boolean;
  pill?: boolean;
  /** "menu" renders the trigger as a ghost toolbar button, like Menu. */
  appearance?: "field" | "menu";
  className?: string;
  heading?: string;
  placeholder?: string;
  align?: "start" | "center" | "end";
  autoFocus?: boolean;
}) {
  const swap = useChangeCount(value);
  const items = options.map(([key, text, hint]) => <SelectPrimitive.Item value={key} key={key} className="r-menu-item">
    <span className="r-menu-copy"><SelectPrimitive.ItemText>{text}</SelectPrimitive.ItemText>{hint && <small>{hint}</small>}</span>
    <SelectPrimitive.ItemIndicator className="r-menu-check"><Check size={13} /></SelectPrimitive.ItemIndicator>
  </SelectPrimitive.Item>);
  return <SelectPrimitive.Root value={value} onValueChange={next => onChange(next as T)} disabled={disabled}>
    <SelectPrimitive.Trigger aria-label={label} data-autofocus={autoFocus || undefined}
      className={appearance === "menu" ? cn("btn btn-ghost r-menu-trigger r-select-button", className) : cn("r-input r-select", pill && "is-pill", className)}>
      <span key={swap} className="r-select-value" data-swap={swap ? "" : undefined}><SelectPrimitive.Value placeholder={placeholder} /></span>
      <SelectPrimitive.Icon className="r-select-icon"><ChevronDown size={appearance === "menu" ? 13 : 14} className="r-caret" /></SelectPrimitive.Icon>
    </SelectPrimitive.Trigger>
    <SelectPrimitive.Portal><SelectPrimitive.Content position="popper" align={align} sideOffset={5} collisionPadding={8} className="r-select-menu"><SelectPrimitive.Viewport>
      {heading ? <SelectPrimitive.Group><SelectPrimitive.Label className="r-menu-heading">{heading}</SelectPrimitive.Label>{items}</SelectPrimitive.Group> : items}
      <GlidePill />
    </SelectPrimitive.Viewport></SelectPrimitive.Content></SelectPrimitive.Portal>
  </SelectPrimitive.Root>;
}

/** Increments whenever value changes after mount, so the first render stays still. */
function useChangeCount(value: unknown): number {
  const [state, setState] = useState({ value, count: 0 });
  if (state.value !== value) setState({ value, count: state.count + 1 });
  return state.value === value ? state.count : state.count + 1;
}

/**
 * Sliding highlight shared by selects, dropdown and context menus. Place it as the last
 * child of a positioned list; it follows whichever item Radix marks [data-highlighted].
 */
export function GlidePill() {
  const ref = useRef<HTMLSpanElement>(null);
  useLayoutEffect(() => {
    const pill = ref.current;
    const host = pill?.parentElement;
    if (!pill || !host) return;
    let shown = false;
    let hideTimer = 0;
    const place = () => {
      const item = host.querySelector<HTMLElement>(".r-menu-item[data-highlighted]");
      window.clearTimeout(hideTimer);
      if (!item) {
        // Radix clears the highlight between leaving one item and entering the next; wait before hiding.
        hideTimer = window.setTimeout(() => { shown = false; pill.dataset.shown = "false"; }, 60);
        return;
      }
      let top = 0, left = 0;
      for (let node: HTMLElement | null = item; node && node !== host; node = node.offsetParent as HTMLElement | null) { top += node.offsetTop; left += node.offsetLeft; }
      // A hidden pill jumps to the item and only fades in; a visible one glides.
      if (!shown) pill.style.transition = "none";
      pill.classList.toggle("is-danger", item.classList.contains("is-danger"));
      pill.style.transform = `translate(${left}px, ${top}px)`;
      pill.style.width = `${item.offsetWidth}px`;
      pill.style.height = `${item.offsetHeight}px`;
      if (!shown) {
        void pill.offsetHeight;
        pill.style.transition = "";
        shown = true;
      }
      pill.dataset.shown = "true";
    };
    place();
    const observer = new MutationObserver(place);
    observer.observe(host, { subtree: true, attributes: true, attributeFilter: ["data-highlighted"] });
    return () => { observer.disconnect(); window.clearTimeout(hideTimer); };
  }, []);
  return <span ref={ref} className="r-glide-pill" aria-hidden="true" />;
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

export function Menu({ label, items, variant = "ghost", disabled, direction = "down", heading, className, trigger }: {
  label?: ReactNode;
  items: MenuItem[];
  variant?: "primary" | "ghost" | "default";
  disabled?: boolean;
  direction?: "down" | "up" | "right";
  heading?: string;
  className?: string;
  /** Replaces the default button; it must forward refs and props. */
  trigger?: ReactElement;
}) {
  return <DropdownMenu.Root><DropdownMenu.Trigger asChild disabled={disabled}>
    {trigger ?? <Button variant={variant} className={cn("r-menu-trigger", className)} disabled={disabled}>{label}<ChevronDown size={13} className="r-caret" /></Button>}
  </DropdownMenu.Trigger><DropdownMenu.Portal>
    <DropdownMenu.Content side={direction === "up" ? "top" : direction === "right" ? "right" : "bottom"} align="end" sideOffset={direction === "right" ? 10 : 6} collisionPadding={8} className="r-menu">
      {heading && <DropdownMenu.Label className="r-menu-heading">{heading}</DropdownMenu.Label>}
      {items.map(item => <MenuEntry key={item.label} item={item} />)}
      <GlidePill />
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

/** Right-click counterpart of Menu; renders children unchanged when there are no items. */
export function RightClickMenu({ items, children }: { items?: MenuItem[]; children: ReactElement }) {
  if (!items?.length) return children;
  return <ContextMenu.Root><ContextMenu.Trigger asChild>{children}</ContextMenu.Trigger><ContextMenu.Portal>
    <ContextMenu.Content className="r-menu" collisionPadding={8}>
      {items.map(item => <Fragment key={item.label}>{item.separator && <ContextMenu.Separator className="r-menu-separator" />}
        <ContextMenu.Item disabled={item.disabled} className={cn("r-menu-item", item.danger && "is-danger")} onSelect={item.action}><span className="r-menu-copy"><span>{item.label}</span>{item.hint && <small>{item.hint}</small>}</span></ContextMenu.Item>
      </Fragment>)}
      <GlidePill />
    </ContextMenu.Content>
  </ContextMenu.Portal></ContextMenu.Root>;
}

export function Modal({ open, onClose, title, description, busy = false, width = 440, children, footer, onEscapeKeyDown }: {
  open: boolean;
  onClose: () => void;
  title: string;
  description?: string;
  busy?: boolean;
  width?: number;
  children: ReactNode;
  footer?: ReactNode;
  onEscapeKeyDown?: (event: KeyboardEvent) => void;
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
      onEscapeKeyDown={event => { onEscapeKeyDown?.(event); event.stopPropagation(); if (busy) event.preventDefault(); }}
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
