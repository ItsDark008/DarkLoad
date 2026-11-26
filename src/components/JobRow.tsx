import { memo } from "react";
import { toast } from "sonner";
import {
  ArrowDown,
  ArrowUp,
  ChevronsUp,
  Copy,
  ExternalLink,
  FolderOpen,
  MoreHorizontal,
  Pause,
  Play,
  RotateCcw,
  Trash2,
  X,
} from "lucide-react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { PlatformIcon } from "@/components/brand";
import { StatusBadge } from "@/components/common";
import { Thumb } from "@/components/Thumb";
import { Button } from "@/components/ui/button";
import { Progress, Tooltip } from "@/components/ui/controls";
import { ContextMenu, DropdownMenu, type MenuItem } from "@/components/ui/overlays";
import { formatBytes, formatEta, formatPercent, formatSpeed } from "@/lib/format";
import { errorHint, isFinished, isRunning } from "@/lib/options";
import { cn } from "@/lib/utils";
import { api } from "@/services/api";
import type { Job } from "@/types/api";

/** Spotify source details are information, not warnings. */
function isInfoNote(note: string): boolean {
  return note.startsWith("Audio source on YouTube:") || note.startsWith("Matched automatically by title");
}

function run(action: Promise<unknown>, failure: string) {
  action.catch((error: { message?: string }) => toast.error(failure, { description: error.message }));
}

interface JobRowProps {
  job: Job;
  index: number;
  count: number;
  compact?: boolean;
}

export const JobRow = memo(function JobRow({ job, index, count, compact }: JobRowProps) {
  const running = isRunning(job.state);
  const finished = isFinished(job.state);
  const processing = job.state === "merging" || job.state === "converting";
  const p = job.progress;
  const audio = job.request.options.mediaType === "audio";

  const items: MenuItem[] = [
    ...(job.state === "completed"
      ? [
          { label: "Open file", icon: <ExternalLink />, onSelect: () => run(api.queueOpen(job.id), "Could not open the file") },
          { label: "Show in folder", icon: <FolderOpen />, onSelect: () => run(api.queueReveal(job.id), "Could not open the folder") },
        ]
      : []),
    ...(running || job.state === "queued"
      ? [{ label: "Pause", icon: <Pause />, onSelect: () => run(api.queuePause(job.id), "Could not pause") }]
      : []),
    ...(job.state === "pending" ? [{ label: "Resume", icon: <Play />, onSelect: () => run(api.queueResume(job.id), "Could not resume") }] : []),
    ...(job.state === "failed" || job.state === "cancelled"
      ? [{ label: "Retry", icon: <RotateCcw />, onSelect: () => run(api.queueRetry(job.id), "Could not retry") }]
      : []),
    {
      label: "Copy source link",
      icon: <Copy />,
      onSelect: () => run(writeText(job.request.url).then(() => toast.success("Link copied")), "Could not copy the link"),
    },
    ...(!finished
      ? [
          { label: "Move to top", icon: <ChevronsUp />, onSelect: () => run(api.queueMove(job.id, 0), "Could not reorder"), disabled: index === 0, separatorBefore: true },
          { label: "Move up", icon: <ArrowUp />, onSelect: () => run(api.queueMove(job.id, index - 1), "Could not reorder"), disabled: index === 0 },
          { label: "Move down", icon: <ArrowDown />, onSelect: () => run(api.queueMove(job.id, index + 1), "Could not reorder"), disabled: index >= count - 1 },
        ]
      : []),
    ...(!finished
      ? [{ label: "Cancel download", icon: <X />, danger: true, separatorBefore: true, onSelect: () => run(api.queueCancel(job.id), "Could not cancel") }]
      : [{ label: "Remove from queue", icon: <Trash2 />, danger: true, separatorBefore: true, onSelect: () => run(api.queueRemove([job.id]), "Could not remove") }]),
  ];

  const sizeText =
    job.state === "completed"
      ? formatBytes(job.fileSize)
      : p.totalBytes
        ? `${formatBytes(p.downloadedBytes)} of ${formatBytes(p.totalBytes)}`
        : p.downloadedBytes > 0
          ? formatBytes(p.downloadedBytes)
          : null;

  return (
    <ContextMenu items={items}>
      <div
        className={cn(
          "group flex items-start gap-3.5 rounded-lg border border-line bg-card transition-colors hover:border-line-strong",
          compact ? "p-2.5" : "p-3",
        )}
        onKeyDown={(e) => {
          if (e.altKey && e.key === "ArrowUp" && index > 0) run(api.queueMove(job.id, index - 1), "Could not reorder");
          if (e.altKey && e.key === "ArrowDown" && index < count - 1) run(api.queueMove(job.id, index + 1), "Could not reorder");
          if (e.key === "Delete") run(finished ? api.queueRemove([job.id]) : api.queueCancel(job.id), "Action failed");
        }}
        tabIndex={0}
        aria-label={`${job.title}, ${job.state}`}
      >
        <Thumb local={job.thumbnailLocal} remote={job.thumbnail} audio={audio} className={compact ? "h-11 w-[78px] shrink-0" : "h-14 w-[100px] shrink-0"} />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <PlatformIcon platform={job.platform} className="size-3.5 text-fg-2" />
            <span className="truncate text-[13px] font-medium text-fg" title={job.title} data-selectable>
              {job.title}
            </span>
          </div>
          <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-fg-2">
            <StatusBadge state={job.state} />
            {job.plan?.qualityLabel ? <span className="truncate">{job.plan.qualityLabel}</span> : null}
            {sizeText ? <span className="tabular">· {sizeText}</span> : null}
          </div>

          {!finished && job.state !== "pending" ? (
            <div className="mt-2.5 space-y-1.5">
              <div className="flex items-center gap-3">
                <Progress
                  value={job.state === "queued" ? 0 : processing ? 1 : p.downloadFraction}
                  tone={processing ? "success" : "accent"}
                  label="Download progress"
                />
                <span className="tabular w-12 shrink-0 text-right text-[11px] text-fg-2">
                  {job.state === "queued" ? "" : formatPercent(processing ? 1 : p.downloadFraction)}
                </span>
              </div>
              {processing ? (
                <div className="flex items-center gap-3">
                  <Progress value={p.processFraction} label={p.processLabel ?? "Processing"} />
                  <span className="tabular w-12 shrink-0 text-right text-[11px] text-fg-2">{formatPercent(p.processFraction)}</span>
                </div>
              ) : null}
              <div className="tabular flex flex-wrap gap-x-3 text-[11px] text-fg-2">
                {job.state === "queued" ? <span>Waiting for a free slot</span> : null}
                {job.state === "analyzing" ? <span>Reading available formats…</span> : null}
                {job.state === "downloading" && p.speed ? <span>{formatSpeed(p.speed)}</span> : null}
                {job.state === "downloading" && p.eta != null ? <span>{formatEta(p.eta)} left</span> : null}
                {processing ? <span>{p.processLabel ?? "Processing"}{p.processEta != null ? ` · ${formatEta(p.processEta)} left` : ""}</span> : null}
                {p.retryLabel ? <span className="text-warning">{p.retryLabel}</span> : null}
              </div>
            </div>
          ) : null}

          {job.state === "failed" && job.error ? (
            <p className="mt-1.5 text-xs leading-relaxed text-danger" data-selectable>
              {job.error.message}
              {errorHint(job.error.code) ? <span className="text-fg-2"> {errorHint(job.error.code)}</span> : null}
            </p>
          ) : null}
          {job.notes.length > 0 && !compact ? (
            <ul className="mt-1.5 space-y-0.5 text-xs">
              {job.notes.map((note) => (
                <li key={note} className={isInfoNote(note) ? "break-all text-fg-2" : "text-warning/90"} data-selectable>
                  {note}
                </li>
              ))}
            </ul>
          ) : null}
        </div>

        <div className="flex shrink-0 items-center gap-1">
          {running || job.state === "queued" ? (
            <Tooltip content="Pause">
              <Button size="icon-sm" variant="ghost" aria-label="Pause" onClick={() => run(api.queuePause(job.id), "Could not pause")}>
                <Pause />
              </Button>
            </Tooltip>
          ) : null}
          {job.state === "pending" ? (
            <Tooltip content="Resume">
              <Button size="icon-sm" variant="ghost" aria-label="Resume" onClick={() => run(api.queueResume(job.id), "Could not resume")}>
                <Play />
              </Button>
            </Tooltip>
          ) : null}
          {job.state === "failed" || job.state === "cancelled" ? (
            <Tooltip content="Retry">
              <Button size="icon-sm" variant="ghost" aria-label="Retry" onClick={() => run(api.queueRetry(job.id), "Could not retry")}>
                <RotateCcw />
              </Button>
            </Tooltip>
          ) : null}
          {job.state === "completed" ? (
            <Tooltip content="Show in folder">
              <Button size="icon-sm" variant="ghost" aria-label="Show in folder" onClick={() => run(api.queueReveal(job.id), "Could not open the folder")}>
                <FolderOpen />
              </Button>
            </Tooltip>
          ) : null}
          {!finished ? (
            <Tooltip content="Cancel">
              <Button size="icon-sm" variant="ghost" aria-label="Cancel download" onClick={() => run(api.queueCancel(job.id), "Could not cancel")}>
                <X />
              </Button>
            </Tooltip>
          ) : null}
          <DropdownMenu
            items={items}
            trigger={
              <Button size="icon-sm" variant="ghost" aria-label="More actions">
                <MoreHorizontal />
              </Button>
            }
          />
        </div>
      </div>
    </ContextMenu>
  );
});
