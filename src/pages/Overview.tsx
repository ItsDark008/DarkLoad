import { useEffect, useState } from "react";
import { ArrowDownToLine, CheckCircle2, HardDrive, History as HistoryIcon, ListVideo, Search, XCircle, type LucideIcon } from "lucide-react";
import { JobRow } from "@/components/JobRow";
import { EmptyState, PageHeader, StatusBadge } from "@/components/common";
import { PlatformIcon } from "@/components/brand";
import { Thumb } from "@/components/Thumb";
import { UrlField } from "@/components/UrlField";
import { Button } from "@/components/ui/button";
import { Card, SectionTitle, Skeleton } from "@/components/ui/controls";
import { formatBytes, formatCount, formatRelative } from "@/lib/format";
import { isRunning } from "@/lib/options";
import { PLATFORM_NAMES } from "@/lib/urls";
import { cn } from "@/lib/utils";
import { api } from "@/services/api";
import { depsHealth, useApp } from "@/stores/app";
import { useHistorySignal } from "@/stores/signals";
import { summarize, useQueue } from "@/stores/queue";
import type { HistoryStats, JobState, Platform } from "@/types/api";

function Stat({ label, value, icon: Icon, tone, hint }: { label: string; value: string; icon: LucideIcon; tone?: string; hint?: string }) {
  return (
    <Card className="p-4">
      <div className="flex items-center justify-between text-xs text-fg-2">
        {label}
        <Icon className={cn("size-4", tone ?? "text-fg-2")} strokeWidth={1.8} />
      </div>
      <div className="tabular mt-2 text-[22px] font-semibold tracking-tight text-fg">{value}</div>
      {hint ? <div className="mt-0.5 text-xs text-muted">{hint}</div> : null}
    </Card>
  );
}

const PLATFORM_CONTENT: Record<Platform, string> = {
  youtube: "Videos, Shorts, playlists, archived streams",
  tiktok: "Public videos and short links",
  instagram: "Public reels, posts and carousels",
};

export function OverviewPage() {
  const openInDownloader = useApp((s) => s.openInDownloader);
  const navigate = useApp((s) => s.navigate);
  const deps = useApp((s) => s.deps);
  const depsLoaded = useApp((s) => s.depsLoaded);
  const installDeps = useApp((s) => s.installDeps);
  const installing = useApp((s) => s.installing);
  const jobs = useQueue((s) => s.jobs);
  const historyVersion = useHistorySignal((s) => s.version);
  const [stats, setStats] = useState<HistoryStats | null>(null);
  const [url, setUrl] = useState("");

  useEffect(() => {
    let alive = true;
    api
      .historyStats()
      .then((s) => alive && setStats(s))
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [historyVersion]);

  const queue = summarize(jobs);
  const activeJobs = jobs.filter((j) => isRunning(j.state) || j.state === "queued");
  const health = depsLoaded ? depsHealth(deps) : "unknown";
  const missing = deps.filter((d) => !d.available).map((d) => d.id);
  const countFor = (p: Platform) => stats?.byPlatform.find((b) => b.platform === p)?.count ?? 0;

  return (
    <div className="mx-auto max-w-[1180px] px-8 py-7">
      <PageHeader title="Overview" description="Your downloads at a glance." />

      <UrlField value={url} onChange={setUrl} onSubmit={(u) => openInDownloader(u)} action={<Button type="submit" variant="primary" disabled={!url.trim()}><Search /> Analyze</Button>} />

      <div className="mt-6 grid grid-cols-2 gap-3 xl:grid-cols-4">
        <Stat label="Active downloads" value={formatCount(queue.active)} icon={ArrowDownToLine} tone={queue.active ? "text-accent-hover" : undefined} hint={queue.paused ? `${queue.paused} paused` : undefined} />
        <Stat label="Completed" value={stats ? formatCount(stats.completed) : "—"} icon={CheckCircle2} tone="text-success" />
        <Stat label="Failed" value={stats ? formatCount(stats.failed) : "—"} icon={XCircle} tone={stats?.failed ? "text-danger" : undefined} />
        <Stat label="Total downloaded" value={stats ? formatBytes(stats.totalBytes) : "—"} icon={HardDrive} />
      </div>

      <div className="mt-6 grid grid-cols-1 gap-6 xl:grid-cols-[minmax(0,1fr)_340px]">
        <div className="min-w-0 space-y-6">
          <section>
            <SectionTitle action={activeJobs.length > 0 ? <Button size="sm" variant="ghost" onClick={() => navigate("queue")}>View queue</Button> : null}>
              Active downloads
            </SectionTitle>
            {activeJobs.length > 0 ? (
              <div className="space-y-2">
                {activeJobs.slice(0, 4).map((job) => (
                  <JobRow key={job.id} job={job} index={jobs.indexOf(job)} count={jobs.length} compact />
                ))}
                {activeJobs.length > 4 ? <p className="text-xs text-fg-2">and {activeJobs.length - 4} more in the queue</p> : null}
              </div>
            ) : (
              <Card>
                <EmptyState icon={ListVideo} title="Nothing downloading" description="Downloads you start appear here with live progress." />
              </Card>
            )}
          </section>

          <section>
            <SectionTitle action={stats && stats.recent.length > 0 ? <Button size="sm" variant="ghost" onClick={() => navigate("history")}>Open history</Button> : null}>
              Recent activity
            </SectionTitle>
            {!stats ? (
              <Card className="space-y-3 p-4">
                <Skeleton className="h-10 w-full" />
                <Skeleton className="h-10 w-full" />
              </Card>
            ) : stats.recent.length === 0 ? (
              <Card>
                <EmptyState icon={HistoryIcon} title="No downloads yet" description="Finished, failed and cancelled downloads are recorded here on this device only." />
              </Card>
            ) : (
              <Card className="divide-y divide-line">
                {stats.recent.map((entry) => (
                  <div key={entry.id} className="flex items-center gap-3 px-4 py-2.5">
                    <Thumb local={entry.thumbnailLocal} remote={entry.thumbnail} audio={entry.mediaType === "audio"} className="aspect-video h-9 shrink-0" />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-[13px] text-fg">{entry.title}</div>
                      <div className="flex items-center gap-1.5 text-xs text-fg-2">
                        <PlatformIcon platform={entry.platform} className="size-3" />
                        {entry.quality ?? entry.format ?? entry.mediaType}
                      </div>
                    </div>
                    <StatusBadge state={entry.status as JobState} />
                    <span className="w-24 shrink-0 text-right text-xs text-fg-2">{formatRelative(entry.createdAt)}</span>
                  </div>
                ))}
              </Card>
            )}
          </section>
        </div>

        <div className="space-y-6">
          <section>
            <SectionTitle>Supported platforms</SectionTitle>
            <div className="space-y-2">
              {(["youtube", "tiktok", "instagram"] as Platform[]).map((p) => (
                <button
                  key={p}
                  type="button"
                  onClick={() => navigate("downloader")}
                  className="flex w-full items-center gap-3 rounded-lg border border-line bg-card px-4 py-3 text-left transition-colors hover:border-line-strong hover:bg-elevated"
                >
                  <PlatformIcon platform={p} className="size-5 text-fg" />
                  <div className="min-w-0 flex-1">
                    <div className="text-[13px] font-medium">{PLATFORM_NAMES[p]}</div>
                    <div className="truncate text-xs text-fg-2">{PLATFORM_CONTENT[p]}</div>
                  </div>
                  <span className="tabular text-xs text-fg-2">{stats ? `${formatCount(countFor(p))} saved` : ""}</span>
                </button>
              ))}
            </div>
          </section>

          <section>
            <SectionTitle action={<Button size="sm" variant="ghost" onClick={() => navigate("settings", "dependencies")}>Manage</Button>}>Components</SectionTitle>
            <Card className="divide-y divide-line">
              {!depsLoaded ? (
                <div className="space-y-2 p-4">
                  <Skeleton className="h-4 w-full" />
                  <Skeleton className="h-4 w-full" />
                </div>
              ) : (
                deps.map((d) => (
                  <div key={d.id} className="flex items-center gap-3 px-4 py-2.5">
                    <span className={cn("size-2 rounded-full", d.available ? "bg-success" : d.required ? "bg-danger" : "bg-warning")} aria-hidden />
                    <span className="flex-1 text-[13px]">{d.name}</span>
                    <span className="max-w-[150px] truncate font-mono text-[11px] text-fg-2">{d.available ? d.version : d.required ? "Missing" : "Optional"}</span>
                  </div>
                ))
              )}
              {health === "error" || (health === "warning" && missing.length > 0) ? (
                <div className="p-3">
                  <Button variant="primary" className="w-full" disabled={installing} onClick={() => void installDeps(missing.length ? missing : ["yt-dlp", "ffmpeg", "deno"])}>
                    {installing ? "Installing…" : "Install missing components"}
                  </Button>
                </div>
              ) : null}
            </Card>
          </section>
        </div>
      </div>
    </div>
  );
}
