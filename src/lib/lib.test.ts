import { describe, expect, it } from "vitest";
import { codecLabel, formatBytes, formatDuration, formatEta, formatPercent, formatRelative, formatSpeed } from "./format";
import { dedupeUrls, detectPlatform, extractUrls, urlKey } from "./urls";
import { contrastRatio, isValidOledBackground, lighten, oledPalette, readableTextOn } from "./utils";
import { errorHint, isFinished, isRunning, optionsForPlatform, optionsFromSettings } from "./options";
import type { DownloadOptions, Settings } from "@/types/api";

describe("formatBytes", () => {
  it("formats across units", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(5 * 1024 ** 3)).toBe("5.0 GB");
    expect(formatBytes(150 * 1024 ** 2)).toBe("150 MB");
  });
  it("handles missing values", () => {
    expect(formatBytes(null)).toBe("—");
    expect(formatBytes(-1)).toBe("—");
    expect(formatBytes(Number.NaN)).toBe("—");
  });
});

describe("time formatting", () => {
  it("formats durations", () => {
    expect(formatDuration(5)).toBe("0:05");
    expect(formatDuration(635)).toBe("10:35");
    expect(formatDuration(3723)).toBe("1:02:03");
    expect(formatDuration(null)).toBe("—");
  });
  it("formats ETAs", () => {
    expect(formatEta(42)).toBe("42s");
    expect(formatEta(125)).toBe("2m 05s");
    expect(formatEta(7260)).toBe("2h 01m");
  });
  it("formats relative times", () => {
    const now = 1_700_000_000_000;
    expect(formatRelative(now - 10_000, now)).toBe("just now");
    expect(formatRelative(now - 5 * 60_000, now)).toBe("5 minutes ago");
    expect(formatRelative(now - 3 * 3600_000, now)).toBe("3 hours ago");
  });
  it("formats speeds and percentages", () => {
    expect(formatSpeed(2048)).toBe("2.0 KB/s");
    expect(formatSpeed(0)).toBe("—");
    expect(formatPercent(0.4567)).toBe("45.6%");
    expect(formatPercent(1.2)).toBe("100%");
  });
  it("labels codecs", () => {
    expect(codecLabel("av1")).toBe("AV1");
    expect(codecLabel("xyz")).toBe("XYZ");
  });
});

describe("platform detection", () => {
  it("detects supported hosts", () => {
    expect(detectPlatform("https://www.youtube.com/watch?v=dQw4w9WgXcQ")).toBe("youtube");
    expect(detectPlatform("youtu.be/dQw4w9WgXcQ")).toBe("youtube");
    expect(detectPlatform("https://vm.tiktok.com/ZMabc/")).toBe("tiktok");
    expect(detectPlatform("https://www.instagram.com/reel/abc/")).toBe("instagram");
    expect(detectPlatform("https://open.spotify.com/track/4PTG3Z6ehGkBFwjybzWkR8")).toBe("spotify");
    expect(detectPlatform("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M")).toBe("spotify");
    expect(detectPlatform("https://spotify.link/AbCdEf")).toBe("spotify");
  });
  it("rejects lookalikes and junk", () => {
    expect(detectPlatform("https://youtube.com.evil.example/watch")).toBeNull();
    expect(detectPlatform("https://notyoutube.com/watch")).toBeNull();
    expect(detectPlatform("https://open.spotify.com.evil.example/track/x")).toBeNull();
    expect(detectPlatform("ftp://youtube.com/x")).toBeNull();
    expect(detectPlatform("hello world")).toBeNull();
    expect(detectPlatform("")).toBeNull();
  });
});

describe("batch URL parsing", () => {
  it("extracts links from free text", () => {
    const text = `Watch this: https://www.youtube.com/watch?v=aaaaaaaaaaa, and (https://youtu.be/bbbbbbbbbbb).
      tiktok.com/@user/video/1234567890 https://example.com/ignored`;
    expect(extractUrls(text)).toEqual([
      "https://www.youtube.com/watch?v=aaaaaaaaaaa",
      "https://youtu.be/bbbbbbbbbbb",
      "https://tiktok.com/@user/video/1234567890",
    ]);
  });
  it("removes duplicates regardless of tracking parameters", () => {
    const { unique, duplicates } = dedupeUrls([
      "https://www.youtube.com/watch?v=aaaaaaaaaaa",
      "https://youtu.be/aaaaaaaaaaa?si=xyz",
      "https://m.youtube.com/watch?v=aaaaaaaaaaa&feature=share",
      "https://www.instagram.com/reel/ABC/",
      "https://instagram.com/reel/ABC",
    ]);
    expect(unique).toHaveLength(2);
    expect(duplicates).toBe(3);
    expect(urlKey("https://www.youtube.com/shorts/ccccccccccc")).toBe("youtube:ccccccccccc");
  });
  it("finds and deduplicates Spotify links", () => {
    const text = `Album: https://open.spotify.com/intl-it/album/6N9PS4QXF1D0OWPk0Sxtb4?si=abc
      song spotify:track:4PTG3Z6ehGkBFwjybzWkR8 and again open.spotify.com/track/4PTG3Z6ehGkBFwjybzWkR8.`;
    const urls = extractUrls(text);
    expect(urls).toEqual([
      "https://open.spotify.com/intl-it/album/6N9PS4QXF1D0OWPk0Sxtb4?si=abc",
      "spotify:track:4PTG3Z6ehGkBFwjybzWkR8",
      "https://open.spotify.com/track/4PTG3Z6ehGkBFwjybzWkR8",
    ]);
    const { unique, duplicates } = dedupeUrls(urls);
    expect(unique).toHaveLength(2);
    expect(duplicates).toBe(1);
  });
});

describe("color utilities", () => {
  it("lightens colors", () => {
    expect(lighten("#000000", 0.5)).toBe("#808080");
    expect(lighten("#8B5CF6", 0)).toBe("#8B5CF6");
    expect(lighten("bad", 0.5)).toBe("bad");
  });
  it("computes contrast", () => {
    expect(contrastRatio("#FFFFFF", "#000000")).toBeCloseTo(21, 0);
    expect(contrastRatio("#A1A1AA", "#000000")).toBeGreaterThan(4.5);
    expect(readableTextOn("#8B5CF6")).toBe("#FFFFFF");
    expect(readableTextOn("#FDE047")).toBe("#000000");
  });
});

describe("OLED palette", () => {
  it("reproduces the default tokens from pure black", () => {
    expect(oledPalette("#000000")).toEqual({
      "--dl-bg": "#000000",
      "--dl-sidebar": "#000000",
      "--dl-card": "#080808",
      "--dl-elevated": "#101010",
      "--dl-hover": "#141414",
      "--dl-border": "#1C1C1C",
      "--dl-border-strong": "#2A2A2A",
    });
  });
  it("derives tinted surfaces from a custom base", () => {
    const p = oledPalette("#04060e");
    expect(p["--dl-bg"]).toBe("#04060E");
    expect(p["--dl-card"]).not.toBe("#080808");
  });
  it("only accepts very dark backgrounds that keep text readable", () => {
    for (const ok of ["#000000", "#04060E", "#0A0510", "#030A06", "#0E0505", "#0C0C0E"]) expect(isValidOledBackground(ok)).toBe(true);
    for (const bad of ["#3A3A3A", "#1E3A8A", "#FFFFFF", "black", "#12345"]) expect(isValidOledBackground(bad)).toBe(false);
    expect(contrastRatio("#A1A1AA", "#262626")).toBeGreaterThan(4.5);
  });
});

describe("options", () => {
  it("derives defaults from settings", () => {
    const settings = {
      defaultMediaType: "audio",
      defaultQualityMode: "exact",
      defaultContainer: "mkv",
      defaultAudioFormat: "mp3",
      defaultAudioBitrate: 256,
      videoCodec: "any",
      audioCodec: "any",
      allowHdr: true,
      embedMetadata: true,
      embedThumbnail: true,
      writeThumbnail: false,
      embedChapters: true,
      downloadSubtitles: false,
      embedSubtitles: true,
      subtitleLanguages: ["en"],
      includeAutoSubtitles: false,
    } as unknown as Settings;
    const options = optionsFromSettings(settings);
    expect(options.mediaType).toBe("audio");
    expect(options.qualityMode).toBe("best");
    expect(options.audioBitrate).toBe(256);
    expect(options.container).toBe("mkv");
  });
  it("forces audio for Spotify links", () => {
    const video = { mediaType: "video" } as DownloadOptions;
    expect(optionsForPlatform(video, "spotify").mediaType).toBe("audio");
    expect(optionsForPlatform(video, "youtube")).toBe(video);
  });
  it("classifies job states", () => {
    expect(isRunning("merging")).toBe(true);
    expect(isRunning("queued")).toBe(false);
    expect(isFinished("cancelled")).toBe(true);
    expect(errorHint("disk_full")).toMatch(/disk space/);
    expect(errorHint("private_content")).toBeNull();
  });
});
