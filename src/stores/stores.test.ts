import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Job } from "@/types/api";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { jobFraction, summarize, useQueue } from "./queue";
import { useBatch } from "./batch";
import { useDownloader } from "./downloader";
import { depsHealth } from "./app";

function job(id: string, state: Job["state"], patch: Partial<Job["progress"]> = {}): Job {
  return {
    id,
    request: { url: "https://www.youtube.com/watch?v=aaaaaaaaaaa", options: {} as Job["request"]["options"] },
    platform: "youtube",
    state,
    title: id,
    uploader: null,
    thumbnail: null,
    thumbnailLocal: null,
    duration: null,
    outputDir: "/tmp",
    plan: null,
    progress: {
      downloadFraction: null,
      downloadedBytes: 0,
      totalBytes: null,
      speed: null,
      eta: null,
      processLabel: null,
      processFraction: null,
      processEta: null,
      retryLabel: null,
      ...patch,
    },
    error: null,
    notes: [],
    attempts: 0,
    paused: false,
    createdAt: 0,
    startedAt: null,
    finishedAt: null,
    outputPath: null,
    fileSize: null,
  };
}

describe("queue store", () => {
  beforeEach(() => useQueue.setState({ jobs: [], loaded: false }));

  it("upserts jobs in place", () => {
    const { upsertJob } = useQueue.getState();
    upsertJob(job("a", "queued"));
    upsertJob(job("b", "queued"));
    upsertJob(job("a", "downloading", { downloadFraction: 0.5 }));
    const jobs = useQueue.getState().jobs;
    expect(jobs.map((j) => j.id)).toEqual(["a", "b"]);
    expect(jobs[0]?.state).toBe("downloading");
  });

  it("summarizes real progress only", () => {
    const s = summarize([
      job("a", "downloading", { downloadFraction: 0.5, speed: 1000 }),
      job("b", "queued"),
      job("c", "completed"),
      job("d", "failed"),
      job("e", "pending"),
    ]);
    expect(s.running).toBe(1);
    expect(s.queued).toBe(1);
    expect(s.active).toBe(2);
    expect(s.completed).toBe(1);
    expect(s.failed).toBe(1);
    expect(s.paused).toBe(1);
    expect(s.speed).toBe(1000);
    expect(s.fraction).toBeCloseTo((0.5 * 0.85) / 2);
    expect(summarize([]).fraction).toBeNull();
  });

  it("weights processing after downloading", () => {
    expect(jobFraction(job("a", "downloading", { downloadFraction: 1 }))).toBeCloseTo(0.85);
    expect(jobFraction(job("a", "merging", { processFraction: 0.5 }))).toBeCloseTo(0.925);
    expect(jobFraction(job("a", "downloading"))).toBeNull();
    expect(jobFraction(job("a", "completed"))).toBe(1);
  });
});

describe("batch store", () => {
  beforeEach(() => useBatch.setState({ items: [], analyzing: false }));

  it("adds unique links and reports duplicates", () => {
    const { addUrls } = useBatch.getState();
    const first = addUrls(["https://youtu.be/aaaaaaaaaaa", "https://www.youtube.com/watch?v=aaaaaaaaaaa", "https://www.tiktok.com/@a/video/12345678"]);
    expect(first).toEqual({ added: 2, duplicates: 1 });
    const second = addUrls(["https://www.youtube.com/watch?v=aaaaaaaaaaa&si=x"]);
    expect(second).toEqual({ added: 0, duplicates: 1 });
    expect(useBatch.getState().items[1]?.platform).toBe("tiktok");
  });

  it("analyzes items and records errors", async () => {
    const mocked = vi.mocked(invoke);
    mocked.mockReset();
    mocked.mockImplementation(async (_cmd, args) => {
      const url = (args as { url: string }).url;
      if (url.includes("bad")) throw { code: "private_content", message: "This content is private.", detail: null, retryable: false };
      return { kind: "media", title: "ok", formats: [], summary: {} };
    });
    useBatch.getState().addUrls(["https://youtu.be/good0000000", "https://youtu.be/bad00000000"]);
    await useBatch.getState().analyzeAll(2);
    const [good, bad] = useBatch.getState().items;
    expect(good?.status).toBe("ready");
    expect(bad?.status).toBe("error");
    expect(bad?.error?.code).toBe("private_content");
    expect(bad?.selected).toBe(false);
    useBatch.getState().clear("invalid");
    expect(useBatch.getState().items).toHaveLength(1);
  });
});

describe("downloader store", () => {
  it("ignores stale analysis results", async () => {
    const mocked = vi.mocked(invoke);
    mocked.mockReset();
    let resolveFirst: (v: unknown) => void = () => undefined;
    mocked.mockImplementation(async (cmd, args) => {
      if (cmd === "cancel_analysis") return undefined;
      const url = (args as { url: string }).url;
      if (url.endsWith("first000000")) return new Promise((r) => (resolveFirst = r));
      return { kind: "media", title: "second", formats: [], summary: {} };
    });
    const first = useDownloader.getState().analyze("https://youtu.be/first000000");
    await useDownloader.getState().analyze("https://youtu.be/second00000");
    resolveFirst({ kind: "media", title: "first", formats: [], summary: {} });
    await first;
    const state = useDownloader.getState();
    expect(state.status).toBe("ready");
    expect(state.analysis && "title" in state.analysis ? state.analysis.title : null).toBe("second");
    expect(mocked).toHaveBeenCalledWith("cancel_analysis", expect.anything());
  });
});

describe("dependency health", () => {
  it("reports missing required tools as errors", () => {
    const base = { name: "", description: "", version: null, path: null, source: "managed" as const, installedAt: null, error: null, latestVersion: null, updateAvailable: null };
    expect(depsHealth([])).toBe("unknown");
    expect(depsHealth([{ ...base, id: "yt-dlp", required: true, available: false }])).toBe("error");
    expect(depsHealth([{ ...base, id: "deno", required: false, available: false }])).toBe("warning");
    expect(depsHealth([{ ...base, id: "yt-dlp", required: true, available: true }])).toBe("ok");
  });
});
