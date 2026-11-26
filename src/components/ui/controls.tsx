import * as React from "react";
import { Checkbox as CheckboxPrimitive, Switch as SwitchPrimitive, ToggleGroup, Tooltip as TooltipPrimitive } from "radix-ui";
import { Check, Minus } from "lucide-react";
import { cn } from "@/lib/utils";

export function Switch({ className, ...props }: React.ComponentProps<typeof SwitchPrimitive.Root>) {
  return (
    <SwitchPrimitive.Root
      className={cn(
        "peer inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full border border-line-strong bg-elevated p-0.5 transition-colors outline-none",
        "focus-visible:ring-2 focus-visible:ring-accent-hover/70 data-[state=checked]:border-accent data-[state=checked]:bg-accent disabled:cursor-not-allowed disabled:opacity-40",
        className,
      )}
      {...props}
    >
      <SwitchPrimitive.Thumb className="pointer-events-none block size-3.5 rounded-full bg-fg-2 transition-transform duration-150 data-[state=checked]:translate-x-4 data-[state=checked]:bg-white" />
    </SwitchPrimitive.Root>
  );
}

export function Checkbox({ className, ...props }: React.ComponentProps<typeof CheckboxPrimitive.Root>) {
  return (
    <CheckboxPrimitive.Root
      className={cn(
        "peer flex size-4 shrink-0 items-center justify-center rounded-[4px] border border-line-strong bg-card outline-none transition-colors",
        "focus-visible:ring-2 focus-visible:ring-accent-hover/70 data-[state=checked]:border-accent data-[state=checked]:bg-accent data-[state=indeterminate]:border-accent data-[state=indeterminate]:bg-accent disabled:opacity-40",
        className,
      )}
      {...props}
    >
      <CheckboxPrimitive.Indicator className="text-white">
        {props.checked === "indeterminate" ? <Minus className="size-3" strokeWidth={3} /> : <Check className="size-3" strokeWidth={3} />}
      </CheckboxPrimitive.Indicator>
    </CheckboxPrimitive.Root>
  );
}

export const TooltipProvider = TooltipPrimitive.Provider;

interface TooltipProps {
  content: React.ReactNode;
  children: React.ReactNode;
  side?: "top" | "right" | "bottom" | "left";
  shortcut?: string;
  disabled?: boolean;
}

export function Tooltip({ content, children, side = "top", shortcut, disabled }: TooltipProps) {
  if (disabled) return <>{children}</>;
  return (
    <TooltipPrimitive.Root>
      <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content
          side={side}
          sideOffset={6}
          className="z-50 flex max-w-xs items-center gap-2 rounded-md border border-line-strong bg-elevated px-2 py-1 text-xs text-fg shadow-xl shadow-black animate-fade-in"
        >
          {content}
          {shortcut ? <kbd className="rounded border border-line-strong px-1 font-sans text-[10px] text-fg-2">{shortcut}</kbd> : null}
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}

interface SegmentedProps<T extends string> {
  value: T;
  onValueChange: (value: T) => void;
  options: { value: T; label: React.ReactNode; icon?: React.ReactNode; disabled?: boolean }[];
  className?: string;
  size?: "sm" | "md";
  "aria-label": string;
}

export function Segmented<T extends string>({ value, onValueChange, options, className, size = "md", ...rest }: SegmentedProps<T>) {
  return (
    <ToggleGroup.Root
      type="single"
      value={value}
      onValueChange={(v) => v && onValueChange(v as T)}
      aria-label={rest["aria-label"]}
      className={cn("inline-flex rounded-lg border border-line bg-card p-0.5", className)}
    >
      {options.map((option) => (
        <ToggleGroup.Item
          key={option.value}
          value={option.value}
          disabled={option.disabled}
          className={cn(
            "inline-flex flex-1 items-center justify-center gap-1.5 whitespace-nowrap rounded-md font-medium text-fg-2 transition-colors outline-none",
            "hover:text-fg focus-visible:ring-2 focus-visible:ring-accent-hover/70 data-[state=on]:bg-elevated data-[state=on]:text-fg data-[state=on]:shadow-[inset_0_0_0_1px_var(--dl-border-strong)] disabled:opacity-40",
            size === "sm" ? "h-7 px-2.5 text-xs" : "h-8 px-3.5 text-[13px]",
            "[&_svg]:size-4",
          )}
        >
          {option.icon}
          {option.label}
        </ToggleGroup.Item>
      ))}
    </ToggleGroup.Root>
  );
}

interface ProgressProps {
  value: number | null;
  tone?: "accent" | "success" | "warning" | "danger" | "muted";
  className?: string;
  label?: string;
}

const progressTones = {
  accent: "bg-accent",
  success: "bg-success",
  warning: "bg-warning",
  danger: "bg-danger",
  muted: "bg-muted",
};

/** A thin progress bar. `null` renders an indeterminate state. */
export function Progress({ value, tone = "accent", className, label }: ProgressProps) {
  const pct = value == null ? null : Math.min(100, Math.max(0, value * 100));
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={pct == null ? undefined : Math.round(pct)}
      className={cn("relative h-1 w-full overflow-hidden rounded-full bg-elevated", className)}
    >
      {pct == null ? (
        <div className={cn("absolute inset-y-0 w-2/5 rounded-full animate-indeterminate", progressTones[tone])} />
      ) : (
        <div className={cn("h-full rounded-full transition-[width] duration-300 ease-out", progressTones[tone])} style={{ width: `${pct}%` }} />
      )}
    </div>
  );
}

export function Card({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={cn("rounded-xl border border-line bg-card", className)} {...props} />;
}

export function Badge({
  className,
  tone = "neutral",
  ...props
}: React.ComponentProps<"span"> & { tone?: "neutral" | "accent" | "success" | "warning" | "danger" }) {
  const tones = {
    neutral: "border-line-strong text-fg-2",
    accent: "border-accent/40 text-accent-hover bg-accent/10",
    success: "border-success/30 text-success bg-success/10",
    warning: "border-warning/30 text-warning bg-warning/10",
    danger: "border-danger/30 text-danger bg-danger/10",
  };
  return (
    <span
      className={cn(
        "inline-flex h-5 shrink-0 items-center gap-1 rounded-md border px-1.5 text-[11px] font-medium whitespace-nowrap [&_svg]:size-3",
        tones[tone],
        className,
      )}
      {...props}
    />
  );
}

export function Skeleton({ className }: { className?: string }) {
  return <div aria-hidden className={cn("skeleton rounded-md", className)} />;
}

export function Kbd({ children, className }: { children: React.ReactNode; className?: string }) {
  return (
    <kbd
      className={cn(
        "inline-flex h-5 min-w-5 items-center justify-center rounded border border-line-strong bg-elevated px-1 font-sans text-[11px] text-fg-2",
        className,
      )}
    >
      {children}
    </kbd>
  );
}

export function SectionTitle({ children, action, className }: { children: React.ReactNode; action?: React.ReactNode; className?: string }) {
  return (
    <div className={cn("mb-3 flex items-center justify-between gap-3", className)}>
      <h2 className="text-[13px] font-semibold tracking-tight text-fg">{children}</h2>
      {action}
    </div>
  );
}
