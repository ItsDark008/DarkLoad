import { create } from "zustand";
import type { Job } from "@/types/api";
import { isFinished, isRunning } from "@/lib/options";

interface QueueStore {
  jobs: Job[];
  loaded: boolean;
  setJobs: (jobs: Job[]) => void;
  upsertJob: (job: Job) => void;
}

export const useQueue = create<QueueStore>((set) => ({
  jobs: [],
  loaded: false,
  setJobs: (jobs) => set({ jobs, loaded: true }),
  upsertJob: (job) =>
    set((state) => {
      const index = state.jobs.findIndex((j) => j.id === job.id);
      if (index === -1) return { jobs: [...state.jobs, job] };
      const jobs = state.jobs.slice();
      jobs[index] = job;
      return { jobs };
    }),
}));

export interface QueueSummary {
  running: number;
  queued: number;
  paused: number;
  completed: number;
  failed: number;
  cancelled: number;
  active: number;
  /** Overall progress across running and queued jobs, when it can be computed. */
  fraction: number | null;
  speed: number;
}

/** Aggregates queue state for the sidebar, overview and global progress bar. */
export function summarize(jobs: Job[]): QueueSummary {
  const summary: QueueSummary = { running: 0, queued: 0, paused: 0, completed: 0, failed: 0, cancelled: 0, active: 0, fraction: null, speed: 0 };
  let weight = 0;
  let done = 0;
  for (const job of jobs) {
    if (isRunning(job.state)) summary.running += 1;
    else if (job.state === "queued") summary.queued += 1;
    else if (job.state === "pending") summary.paused += 1;
    else if (job.state === "completed") summary.completed += 1;
    else if (job.state === "failed") summary.failed += 1;
    else if (job.state === "cancelled") summary.cancelled += 1;

    if (isRunning(job.state) || job.state === "queued") {
      weight += 1;
      done += jobFraction(job) ?? 0;
      summary.speed += job.progress.speed ?? 0;
    }
  }
  summary.active = summary.running + summary.queued;
  summary.fraction = weight > 0 ? done / weight : null;
  return summary;
}

/** Combined progress for one job: downloading counts for 85%, processing for the rest. */
export function jobFraction(job: Job): number | null {
  if (job.state === "completed") return 1;
  if (isFinished(job.state) || job.state === "queued" || job.state === "analyzing") return 0;
  const download = job.progress.downloadFraction;
  if (job.state === "merging" || job.state === "converting") {
    return 0.85 + 0.15 * (job.progress.processFraction ?? 0);
  }
  return download == null ? null : download * 0.85;
}
