import { Component, type ErrorInfo, type ReactNode } from "react";
import { AlertTriangle, Ban, CheckCircle2, Clock, Loader2, PauseCircle, Scissors, Sparkles, XCircle, type LucideIcon } from "lucide-react";
import { Badge } from "@/components/ui/controls";
import { Button } from "@/components/ui/button";
import { STATE_LABELS } from "@/lib/options";
import { cn } from "@/lib/utils";
import type { JobState } from "@/types/api";

const STATE_META: Record<JobState, { tone: "neutral" | "accent" | "success" | "warning" | "danger"; icon: LucideIcon; spin?: boolean }> = {
  pending: { tone: "neutral", icon: PauseCircle },
  analyzing: { tone: "accent", icon: Loader2, spin: true },
  queued: { tone: "neutral", icon: Clock },
  downloading: { tone: "accent", icon: Loader2, spin: true },
  merging: { tone: "accent", icon: Scissors },
  converting: { tone: "accent", icon: Sparkles },
  completed: { tone: "success", icon: CheckCircle2 },
  failed: { tone: "danger", icon: XCircle },
  cancelled: { tone: "warning", icon: Ban },
};

export function StatusBadge({ state, className }: { state: JobState; className?: string }) {
  const meta = STATE_META[state];
  const Icon = meta.icon;
  return (
    <Badge tone={meta.tone} className={className}>
      <Icon className={cn(meta.spin && "animate-spin")} />
      {STATE_LABELS[state]}
    </Badge>
  );
}

export function EmptyState({
  icon: Icon,
  title,
  description,
  action,
  className,
}: {
  icon: LucideIcon;
  title: string;
  description: ReactNode;
  action?: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-col items-center justify-center px-6 py-14 text-center", className)}>
      <div className="mb-4 flex size-12 items-center justify-center rounded-xl border border-line bg-card text-fg-2">
        <Icon className="size-5" strokeWidth={1.6} />
      </div>
      <h3 className="text-[14px] font-semibold text-fg">{title}</h3>
      <p className="mt-1.5 max-w-sm text-[13px] leading-relaxed text-fg-2">{description}</p>
      {action ? <div className="mt-5">{action}</div> : null}
    </div>
  );
}

export function PageHeader({ title, description, actions }: { title: string; description?: ReactNode; actions?: ReactNode }) {
  return (
    <div className="mb-6 flex flex-wrap items-end justify-between gap-4">
      <div className="min-w-0">
        <h1 className="text-[20px] font-semibold tracking-tight text-fg">{title}</h1>
        {description ? <p className="mt-1 text-[13px] text-fg-2">{description}</p> : null}
      </div>
      {actions ? <div className="flex flex-wrap items-center gap-2">{actions}</div> : null}
    </div>
  );
}

export function InlineAlert({
  tone = "warning",
  title,
  children,
  action,
  className,
}: {
  tone?: "warning" | "danger" | "info";
  title: string;
  children?: ReactNode;
  action?: ReactNode;
  className?: string;
}) {
  const tones = {
    warning: "border-warning/30 bg-warning/[0.06] [&>svg]:text-warning",
    danger: "border-danger/30 bg-danger/[0.06] [&>svg]:text-danger",
    info: "border-accent/30 bg-accent/[0.06] [&>svg]:text-accent-hover",
  };
  return (
    <div role={tone === "danger" ? "alert" : "status"} className={cn("flex items-start gap-3 rounded-lg border px-3.5 py-3", tones[tone], className)}>
      <AlertTriangle className="mt-0.5 size-4 shrink-0" />
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-medium text-fg">{title}</div>
        {children ? <div className="mt-0.5 text-[13px] leading-relaxed text-fg-2">{children}</div> : null}
      </div>
      {action ? <div className="shrink-0">{action}</div> : null}
    </div>
  );
}

interface BoundaryState {
  error: Error | null;
}

export class ErrorBoundary extends Component<{ children: ReactNode; fallbackTitle?: string }, BoundaryState> {
  override state: BoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): BoundaryState {
    return { error };
  }

  override componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("DarkLoad UI error", error, info.componentStack);
  }

  override render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="flex h-full items-center justify-center p-8">
        <div className="max-w-md rounded-xl border border-line bg-card p-6 text-center">
          <XCircle className="mx-auto mb-3 size-6 text-danger" />
          <h2 className="text-[15px] font-semibold">{this.props.fallbackTitle ?? "This view ran into a problem"}</h2>
          <p className="mt-2 text-[13px] text-fg-2" data-selectable>
            {this.state.error.message}
          </p>
          <Button className="mt-5" variant="primary" onClick={() => this.setState({ error: null })}>
            Try again
          </Button>
        </div>
      </div>
    );
  }
}
