import * as React from "react";
import { AlertDialog as AlertPrimitive, ContextMenu as ContextPrimitive, Dialog as DialogPrimitive, DropdownMenu as DropdownPrimitive } from "radix-ui";
import { X } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "./button";

export const Dialog = DialogPrimitive.Root;

export function DialogContent({
  className,
  children,
  title,
  description,
  footer,
  ...props
}: Omit<React.ComponentProps<typeof DialogPrimitive.Content>, "title"> & {
  title: React.ReactNode;
  description?: React.ReactNode;
  footer?: React.ReactNode;
}) {
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/75 animate-fade-in" />
      <DialogPrimitive.Content
        className={cn(
          "fixed top-1/2 left-1/2 z-50 flex max-h-[85vh] w-[min(560px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 flex-col rounded-xl border border-line-strong bg-card shadow-2xl shadow-black outline-none animate-scale-in",
          className,
        )}
        {...props}
      >
        <div className="flex items-start justify-between gap-4 border-b border-line px-5 py-4">
          <div className="min-w-0">
            <DialogPrimitive.Title className="text-[15px] font-semibold text-fg">{title}</DialogPrimitive.Title>
            <DialogPrimitive.Description className={cn("mt-1 text-[13px] text-fg-2", !description && "sr-only")}>
              {description ?? (typeof title === "string" ? title : "Dialog")}
            </DialogPrimitive.Description>
          </div>
          <DialogPrimitive.Close asChild>
            <Button variant="ghost" size="icon-sm" aria-label="Close">
              <X />
            </Button>
          </DialogPrimitive.Close>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">{children}</div>
        {footer ? <div className="flex justify-end gap-2 border-t border-line px-5 py-3">{footer}</div> : null}
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}

interface ConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description: React.ReactNode;
  confirmLabel: string;
  destructive?: boolean;
  onConfirm: () => void;
  children?: React.ReactNode;
}

export function ConfirmDialog({ open, onOpenChange, title, description, confirmLabel, destructive, onConfirm, children }: ConfirmDialogProps) {
  return (
    <AlertPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <AlertPrimitive.Portal>
        <AlertPrimitive.Overlay className="fixed inset-0 z-50 bg-black/75 animate-fade-in" />
        <AlertPrimitive.Content className="fixed top-1/2 left-1/2 z-50 w-[min(440px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-line-strong bg-card p-5 shadow-2xl shadow-black outline-none animate-scale-in">
          <AlertPrimitive.Title className="text-[15px] font-semibold text-fg">{title}</AlertPrimitive.Title>
          <AlertPrimitive.Description className="mt-2 text-[13px] leading-relaxed text-fg-2">{description}</AlertPrimitive.Description>
          {children}
          <div className="mt-5 flex justify-end gap-2">
            <AlertPrimitive.Cancel asChild>
              <Button variant="ghost">Cancel</Button>
            </AlertPrimitive.Cancel>
            <AlertPrimitive.Action asChild>
              <Button variant={destructive ? "danger" : "primary"} onClick={onConfirm}>
                {confirmLabel}
              </Button>
            </AlertPrimitive.Action>
          </div>
        </AlertPrimitive.Content>
      </AlertPrimitive.Portal>
    </AlertPrimitive.Root>
  );
}

const menuContentClass =
  "z-50 min-w-48 overflow-hidden rounded-lg border border-line-strong bg-elevated p-1 text-[13px] text-fg shadow-2xl shadow-black animate-scale-in";
const menuItemClass =
  "flex cursor-default items-center gap-2.5 rounded-md px-2.5 py-1.5 outline-none select-none data-[disabled]:opacity-40 data-[highlighted]:bg-hover [&_svg]:size-4 [&_svg]:text-fg-2";

export interface MenuItem {
  label: string;
  icon?: React.ReactNode;
  onSelect: () => void;
  disabled?: boolean;
  danger?: boolean;
  separatorBefore?: boolean;
  shortcut?: string;
}

export function DropdownMenu({ trigger, items, align = "end" }: { trigger: React.ReactNode; items: MenuItem[]; align?: "start" | "end" }) {
  return (
    <DropdownPrimitive.Root>
      <DropdownPrimitive.Trigger asChild>{trigger}</DropdownPrimitive.Trigger>
      <DropdownPrimitive.Portal>
        <DropdownPrimitive.Content align={align} sideOffset={6} className={menuContentClass}>
          {items.map((item) => (
            <React.Fragment key={item.label}>
              {item.separatorBefore ? <DropdownPrimitive.Separator className="my-1 h-px bg-line" /> : null}
              <DropdownPrimitive.Item
                disabled={item.disabled}
                onSelect={item.onSelect}
                className={cn(menuItemClass, item.danger && "text-danger [&_svg]:text-danger")}
              >
                {item.icon}
                <span className="flex-1">{item.label}</span>
                {item.shortcut ? <span className="text-xs text-muted">{item.shortcut}</span> : null}
              </DropdownPrimitive.Item>
            </React.Fragment>
          ))}
        </DropdownPrimitive.Content>
      </DropdownPrimitive.Portal>
    </DropdownPrimitive.Root>
  );
}

export function ContextMenu({ children, items }: { children: React.ReactNode; items: MenuItem[] }) {
  return (
    <ContextPrimitive.Root>
      <ContextPrimitive.Trigger asChild>{children}</ContextPrimitive.Trigger>
      <ContextPrimitive.Portal>
        <ContextPrimitive.Content className={menuContentClass}>
          {items.map((item) => (
            <React.Fragment key={item.label}>
              {item.separatorBefore ? <ContextPrimitive.Separator className="my-1 h-px bg-line" /> : null}
              <ContextPrimitive.Item
                disabled={item.disabled}
                onSelect={item.onSelect}
                className={cn(menuItemClass, item.danger && "text-danger [&_svg]:text-danger")}
              >
                {item.icon}
                <span className="flex-1">{item.label}</span>
              </ContextPrimitive.Item>
            </React.Fragment>
          ))}
        </ContextPrimitive.Content>
      </ContextPrimitive.Portal>
    </ContextPrimitive.Root>
  );
}
