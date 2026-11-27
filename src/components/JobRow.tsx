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
import { errorHint, isFinished, isRunning, stateLabel } from "@/lib/options";
import { cn } from "@/lib/utils";
import { api } from "@/services/api";
import { t, tr, useT } from "@/i18n";
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
  useT();
  const running = isRunning(job.state);
  const finished = isFinished(job.state);
  const processing = job.state === "merging" || job.state === "converting";
  const p = job.progress;
  const audio = job.request.options.mediaType === "audio";

  const items: MenuItem[] = [
    ...(job.state === "completed"
      ? [
          { label: t("Open file"), icon: <ExternalLink />, onSelect: () => run(api.queueOpen(job.id), t("Could not open the file")) },
          { label: t("Show in folder"), icon: <FolderOpen />, onSelect: () => run(api.queueReveal(job.id), t("Could not open the folder")) },
        ]
      : []),
    ...(running || job.state === "queued"
      ? [{ label: t("Pause"), icon: <Pause />, onSelect: () => run(api.queuePause(job.id), t("Could not pause")) }]
      : []),
    ...(job.state === "pending" ? [{ label: t("Resume"), icon: <Play />, onSelect: () => run(api.queueResume(job.id), t("Could not resume")) }] : []),
    ...(job.state === "failed" || job.state === "cancelled"
      ? [{ label: t("Retry"), icon: <RotateCcw />, onSelect: () => run(api.queueRetry(job.id), t("Could not retry")) }]
      : []),
    {
      label: t("Copy source link"),
      icon: <Copy />,
      onSelect: () => run(writeText(job.request.url).then(() => toast.success(t("Link copied"))), t("Could not copy the link")),
    },
    ...(!finished
      ? [
          { label: t("Move to top"), icon: <ChevronsUp />, onSelect: () => run(api.queueMove(job.id, 0), t("Could not reorder")), disabled: index === 0, separatorBefore: true },
          { label: t("Move up"), icon: <ArrowUp />, onSelect: () => run(api.queueMove(job.id, index - 1), t("Could not reorder")), disabled: index === 0 },
          { label: t("Move down"), icon: <ArrowDown />, onSelect: () => run(api.queueMove(job.id, index + 1), t("Could not reorder")), disabled: index >= count - 1 },
        ]
      : []),
    ...(!finished
      ? [{ label: t("Cancel download"), icon: <X />, danger: true, separatorBefore: true, onSelect: () => run(api.queueCancel(job.id), t("Could not cancel")) }]
      : [{ label: t("Remove from queue"), icon: <Trash2 />, danger: true, separatorBefore: true, onSelect: () => run(api.queueRemove([job.id]), t("Could not remove")) }]),
  ];

  const sizeText =
    job.state === "completed"
      ? formatBytes(job.fileSize)
      : p.totalBytes
        ? t("{done} of {total}", { done: formatBytes(p.downloadedBytes), total: formatBytes(p.totalBytes) })
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
          if (e.altKey && e.key === "ArrowUp" && index > 0) run(api.queueMove(job.id, index - 1), t("Could not reorder"));
          if (e.altKey && e.key === "ArrowDown" && index < count - 1) run(api.queueMove(job.id, index + 1), t("Could not reorder"));
          if (e.key === "Delete") run(finished ? api.queueRemove([job.id]) : api.queueCancel(job.id), t("Action failed"));
        }}
        tabIndex={0}
        aria-label={`${job.title}, ${stateLabel(job.state)}`}
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
                  label={t("Download progress")}
                />
                <span className="tabular w-12 shrink-0 text-right text-[11px] text-fg-2">
                  {job.state === "queued" ? "" : formatPercent(processing ? 1 : p.downloadFraction)}
                </span>
              </div>
              {processing ? (
                <div className="flex items-center gap-3">
                  <Progress value={p.processFraction} label={tr(p.processLabel) ?? t("Processing")} />
                  <span className="tabular w-12 shrink-0 text-right text-[11px] text-fg-2">{formatPercent(p.processFraction)}</span>
                </div>
              ) : null}
              <div className="tabular flex flex-wrap gap-x-3 text-[11px] text-fg-2">
                {job.state === "queued" ? <span>{t("Waiting for a free slot")}</span> : null}
                {job.state === "analyzing" ? <span>{t("Reading available formats…")}</span> : null}
                {job.state === "downloading" && p.speed ? <span>{formatSpeed(p.speed)}</span> : null}
                {job.state === "downloading" && p.eta != null ? <span>{t("{time} left", { time: formatEta(p.eta) })}</span> : null}
                {processing ? <span>{tr(p.processLabel) ?? t("Processing")}{p.processEta != null ? ` · ${t("{time} left", { time: formatEta(p.processEta) })}` : ""}</span> : null}
                {p.retryLabel ? <span className="text-warning">{tr(p.retryLabel)}</span> : null}
              </div>
            </div>
          ) : null}

          {job.state === "failed" && job.error ? (
            <p className="mt-1.5 text-xs leading-relaxed text-danger" data-selectable>
              {tr(job.error.message)}
              {errorHint(job.error.code) ? <span className="text-fg-2"> {errorHint(job.error.code)}</span> : null}
            </p>
          ) : null}
          {job.notes.length > 0 && !compact ? (
            <ul className="mt-1.5 space-y-0.5 text-xs">
              {job.notes.map((note) => (
                <li key={note} className={isInfoNote(note) ? "break-all text-fg-2" : "text-warning/90"} data-selectable>
                  {tr(note)}
                </li>
              ))}
            </ul>
          ) : null}
        </div>

        <div className="flex shrink-0 items-center gap-1">
          {running || job.state === "queued" ? (
            <Tooltip content={t("Pause")}>
              <Button size="icon-sm" variant="ghost" aria-label={t("Pause")} onClick={() => run(api.queuePause(job.id), t("Could not pause"))}>
                <Pause />
              </Button>
            </Tooltip>
          ) : null}
          {job.state === "pending" ? (
            <Tooltip content={t("Resume")}>
              <Button size="icon-sm" variant="ghost" aria-label={t("Resume")} onClick={() => run(api.queueResume(job.id), t("Could not resume"))}>
                <Play />
              </Button>
            </Tooltip>
          ) : null}
          {job.state === "failed" || job.state === "cancelled" ? (
            <Tooltip content={t("Retry")}>
              <Button size="icon-sm" variant="ghost" aria-label={t("Retry")} onClick={() => run(api.queueRetry(job.id), t("Could not retry"))}>
                <RotateCcw />
              </Button>
            </Tooltip>
          ) : null}
          {job.state === "completed" ? (
            <Tooltip content={t("Show in folder")}>
              <Button size="icon-sm" variant="ghost" aria-label={t("Show in folder")} onClick={() => run(api.queueReveal(job.id), t("Could not open the folder"))}>
                <FolderOpen />
              </Button>
            </Tooltip>
          ) : null}
          {!finished ? (
            <Tooltip content={t("Cancel")}>
              <Button size="icon-sm" variant="ghost" aria-label={t("Cancel download")} onClick={() => run(api.queueCancel(job.id), t("Could not cancel"))}>
                <X />
              </Button>
            </Tooltip>
          ) : null}
          <DropdownMenu
            items={items}
            trigger={
              <Button size="icon-sm" variant="ghost" aria-label={t("More actions")}>
                <MoreHorizontal />
              </Button>
            }
          />
        </div>
      </div>
    </ContextMenu>
  );
});
