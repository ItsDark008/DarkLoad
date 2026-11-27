import { useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { toast } from "sonner";
import { ListVideo, Pause, Play, Trash2 } from "lucide-react";
import { JobRow } from "@/components/JobRow";
import { EmptyState, PageHeader } from "@/components/common";
import { Button } from "@/components/ui/button";
import { Card, Progress, Segmented } from "@/components/ui/controls";
import { formatPercent, formatSpeed } from "@/lib/format";
import { isFinished, isRunning } from "@/lib/options";
import { api } from "@/services/api";
import { useApp } from "@/stores/app";
import { summarize, useQueue } from "@/stores/queue";
import { t, tc, useT } from "@/i18n";
import type { Job } from "@/types/api";

type Filter = "all" | "active" | "completed" | "failed";

function matches(job: Job, filter: Filter): boolean {
  switch (filter) {
    case "active":
      return !isFinished(job.state);
    case "completed":
      return job.state === "completed";
    case "failed":
      return job.state === "failed" || job.state === "cancelled";
    default:
      return true;
  }
}

function report(action: Promise<unknown>, failure: string) {
  action.catch((e: { message?: string }) => toast.error(failure, { description: e.message }));
}

export function QueuePage() {
  useT();
  const jobs = useQueue((s) => s.jobs);
  const settings = useApp((s) => s.settings);
  const navigate = useApp((s) => s.navigate);
  const [filter, setFilter] = useState<Filter>("all");
  const summary = summarize(jobs);
  const visible = useMemo(() => jobs.filter((j) => matches(j, filter)), [jobs, filter]);
  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: visible.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 104,
    overscan: 6,
    getItemKey: (i) => visible[i]?.id ?? i,
  });
  const finishedCount = jobs.filter((j) => isFinished(j.state)).length;
  const anyRunning = jobs.some((j) => isRunning(j.state) || j.state === "queued");
  const anyPaused = jobs.some((j) => j.state === "pending");

  return (
    <div className="mx-auto flex h-full max-w-[1180px] flex-col px-8 pt-7">
      <PageHeader
        title={t("Download Queue")}
        description={t("Up to {count} downloads run at the same time. Reorder with Alt+↑/↓ or the item menu.", { count: settings?.maxConcurrentDownloads ?? "…" })}
        actions={
          <>
            {anyRunning ? (
              <Button onClick={() => report(api.queuePauseAll(), t("Could not pause"))}>
                <Pause /> {t("Pause all")}
              </Button>
            ) : null}
            {anyPaused ? (
              <Button onClick={() => report(api.queueResumeAll(), t("Could not resume"))}>
                <Play /> {t("Resume all")}
              </Button>
            ) : null}
            <Button variant="ghost" disabled={finishedCount === 0} onClick={() => report(api.queueClearFinished(), t("Could not clear"))}>
              <Trash2 /> {t("Clear finished")}
            </Button>
          </>
        }
      />

      {jobs.length > 0 ? (
        <Card className="mb-4 flex flex-wrap items-center gap-x-8 gap-y-3 p-4">
          <div className="min-w-[220px] flex-1">
            <div className="mb-2 flex items-center justify-between text-xs text-fg-2">
              <span>{t("Overall progress")}</span>
              <span className="tabular text-fg">{summary.active > 0 ? formatPercent(summary.fraction) : t("Idle")}</span>
            </div>
            <Progress value={summary.active > 0 ? summary.fraction : 0} label={t("Overall progress")} />
          </div>
          {[
            [t("Downloading"), summary.running],
            [t("Queued"), summary.queued],
            [t("Paused"), summary.paused],
            [tc("count", "Completed"), summary.completed],
            [tc("count", "Failed"), summary.failed],
          ].map(([label, value]) => (
            <div key={label as string} className="text-xs text-fg-2">
              {label}
              <div className="tabular mt-0.5 text-[15px] font-semibold text-fg">{value}</div>
            </div>
          ))}
          <div className="text-xs text-fg-2">
            {t("Speed")}
            <div className="tabular mt-0.5 text-[15px] font-semibold text-fg">{summary.speed > 0 ? formatSpeed(summary.speed) : "—"}</div>
          </div>
        </Card>
      ) : null}

      {jobs.length > 0 ? (
        <Segmented<Filter>
          aria-label={t("Filter downloads")}
          size="sm"
          value={filter}
          onValueChange={setFilter}
          className="mb-3 self-start"
          options={[
            { value: "all", label: t("All {count}", { count: jobs.length }) },
            { value: "active", label: t("Active") },
            { value: "completed", label: tc("count", "Completed") },
            { value: "failed", label: t("Failed & cancelled") },
          ]}
        />
      ) : null}

      <div ref={parentRef} className="-mx-1 min-h-0 flex-1 overflow-y-auto px-1 pb-8">
        {jobs.length === 0 ? (
          <Card>
            <EmptyState
              icon={ListVideo}
              title={t("The queue is empty")}
              description={t("Analyze a link in the Downloader or add many at once from Batch Download.")}
              action={
                <Button variant="primary" onClick={() => navigate("downloader")}>
                  {t("Open Downloader")}
                </Button>
              }
            />
          </Card>
        ) : visible.length === 0 ? (
          <p className="py-10 text-center text-[13px] text-fg-2">{t("No downloads match this filter.")}</p>
        ) : (
          <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
            {virtualizer.getVirtualItems().map((row) => {
              const job = visible[row.index];
              if (!job) return null;
              return (
                <div
                  key={row.key}
                  data-index={row.index}
                  ref={virtualizer.measureElement}
                  style={{ position: "absolute", top: 0, left: 0, width: "100%", transform: `translateY(${row.start}px)` }}
                  className="pb-2"
                >
                  <JobRow job={job} index={jobs.indexOf(job)} count={jobs.length} />
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
