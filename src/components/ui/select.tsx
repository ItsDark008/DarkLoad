import * as React from "react";
import { Select as SelectPrimitive } from "radix-ui";
import { Check, ChevronDown } from "lucide-react";
import { cn } from "@/lib/utils";

export interface SelectOption<T extends string> {
  value: T;
  label: React.ReactNode;
  description?: string;
  disabled?: boolean;
}

interface SelectProps<T extends string> {
  value: T;
  onValueChange: (value: T) => void;
  options: SelectOption<T>[];
  placeholder?: string;
  disabled?: boolean;
  className?: string;
  id?: string;
  "aria-label"?: string;
}

export function Select<T extends string>({ value, onValueChange, options, placeholder, disabled, className, id, ...rest }: SelectProps<T>) {
  return (
    <SelectPrimitive.Root value={value} onValueChange={(v) => onValueChange(v as T)} disabled={disabled}>
      <SelectPrimitive.Trigger
        id={id}
        aria-label={rest["aria-label"]}
        className={cn(
          "flex h-9 w-full items-center justify-between gap-2 rounded-md border border-line bg-card px-3 text-left text-[13px] text-fg outline-none transition-colors",
          "hover:border-line-strong focus-visible:border-accent focus-visible:ring-2 focus-visible:ring-accent/25 data-[placeholder]:text-muted disabled:opacity-50",
          className,
        )}
      >
        <span className="truncate">
          <SelectPrimitive.Value placeholder={placeholder} />
        </span>
        <SelectPrimitive.Icon asChild>
          <ChevronDown className="size-4 shrink-0 text-fg-2" />
        </SelectPrimitive.Icon>
      </SelectPrimitive.Trigger>
      <SelectPrimitive.Portal>
        <SelectPrimitive.Content
          position="popper"
          sideOffset={6}
          className="z-50 max-h-[min(360px,var(--radix-select-content-available-height))] min-w-[var(--radix-select-trigger-width)] overflow-hidden rounded-lg border border-line-strong bg-elevated p-1 shadow-2xl shadow-black animate-scale-in"
        >
          <SelectPrimitive.Viewport>
            {options.map((option) => (
              <SelectPrimitive.Item
                key={option.value}
                value={option.value}
                disabled={option.disabled}
                className="relative flex cursor-default items-start gap-2 rounded-md py-2 pr-8 pl-2.5 text-[13px] text-fg outline-none select-none data-[disabled]:opacity-40 data-[highlighted]:bg-hover"
              >
                <div className="min-w-0">
                  <SelectPrimitive.ItemText>{option.label}</SelectPrimitive.ItemText>
                  {option.description ? <div className="mt-0.5 text-xs text-fg-2">{option.description}</div> : null}
                </div>
                <SelectPrimitive.ItemIndicator className="absolute top-2.5 right-2">
                  <Check className="size-3.5 text-accent-hover" />
                </SelectPrimitive.ItemIndicator>
              </SelectPrimitive.Item>
            ))}
          </SelectPrimitive.Viewport>
        </SelectPrimitive.Content>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  );
}
