import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), convertFileSrc: (p: string) => `asset://${p}` }));

import { FormatDetails, PlanSummary } from "./media";
import type { DownloadOptions, MediaInfo, SelectionPlan } from "@/types/api";

const media = {
  id: "x",
  url: "https://www.youtube.com/watch?v=xxxxxxxxxxx",
  title: "Test media",
  uploader: "Uploader",
  duration: 60,
  thumbnail: null,
  thumbnailLocal: null,
  platform: "youtube",
  wasLive: false,
  uploadDate: null,
  viewCount: null,
  chapters: 0,
  formats: [],
  subtitles: [],
  playlistId: null,
  playlistItem: null,
  summary: {
    resolutions: [
      { height: 2160, label: "4K · 2160p", frameRates: [60], hdr: true, codecs: ["av1", "vp9"] },
      { height: 1080, label: "Full HD · 1080p", frameRates: [60, 30], hdr: false, codecs: ["vp9", "h264"] },
    ],
    videoCodecs: ["av1", "vp9", "h264"],
    audioCodecs: ["opus", "aac"],
    audioBitrates: [160, 128],
    hasVideo: true,
    hasAudio: true,
    hasHdr: true,
    maxHeight: 2160,
    maxFps: 60,
  },
} satisfies MediaInfo;

const plan: SelectionPlan = {
  video: null,
  audio: null,
  formatSelector: "137+140",
  formatSort: null,
  outputContainer: "mkv",
  requiresMerge: true,
  audioConversion: null,
  transcodesAudio: false,
  estimatedSize: 10 * 1024 * 1024,
  sizeIsEstimate: true,
  qualityLabel: "1080p60 · H.264 + Opus 160 kbps",
  notes: [{ level: "info", message: "Some note" }],
  containerConflict: { requested: "webm", suggested: "mkv", reason: "WebM cannot store H.264 + Opus without re-encoding." },
  embedThumbnail: true,
  embedSubtitles: false,
};

describe("FormatDetails", () => {
  it("lists only the resolutions the source offers", () => {
    render(<FormatDetails media={media} />);
    const table = screen.getByRole("table");
    const rows = within(table).getAllByRole("row").slice(1);
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent("4K · 2160p");
    expect(rows[0]).toHaveTextContent("HDR");
    expect(rows[1]).toHaveTextContent("60 · 30");
    expect(table).not.toHaveTextContent("4320");
  });
});

describe("PlanSummary", () => {
  it("explains container conflicts and offers a compatible container", () => {
    const onChange = vi.fn();
    render(<PlanSummary plan={plan} options={{ mediaType: "video" } as DownloadOptions} onChange={onChange} />);
    expect(screen.getByText(/WEBM cannot keep these streams/i)).toBeInTheDocument();
    expect(screen.getByText("≈ 10.0 MB")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Save as MKV" }));
    expect(onChange).toHaveBeenCalledWith({ container: "mkv" });
  });
});
