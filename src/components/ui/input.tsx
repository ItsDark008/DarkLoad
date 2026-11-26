import * as React from "react";
import { cn } from "@/lib/utils";

export function Input({ className, type = "text", ...props }: React.ComponentProps<"input">) {
  return (
    <input
      type={type}
      spellCheck={false}
      autoComplete="off"
      className={cn(
        "h-9 w-full min-w-0 rounded-md border border-line bg-card px-3 text-[13px] text-fg placeholder:text-muted transition-colors outline-none",
        "hover:border-line-strong focus-visible:border-accent focus-visible:ring-2 focus-visible:ring-accent/25 focus-visible:outline-none",
        "disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-danger/70",
        className,
      )}
      {...props}
    />
  );
}

export function Textarea({ className, ...props }: React.ComponentProps<"textarea">) {
  return (
    <textarea
      spellCheck={false}
      className={cn(
        "w-full min-w-0 resize-none rounded-md border border-line bg-card px-3 py-2.5 text-[13px] leading-relaxed text-fg placeholder:text-muted outline-none transition-colors",
        "hover:border-line-strong focus-visible:border-accent focus-visible:ring-2 focus-visible:ring-accent/25 focus-visible:outline-none",
        className,
      )}
      {...props}
    />
  );
}

export function Label({ className, ...props }: React.ComponentProps<"label">) {
  return <label className={cn("text-xs font-medium text-fg-2 select-none", className)} {...props} />;
}
