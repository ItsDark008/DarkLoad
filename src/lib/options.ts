import { t } from "@/i18n";
import type { AudioFormat, AudioTrackSelection, Container, DownloadOptions, ErrorCode, JobState, QualityMode, Settings } from "@/types/api";

export const AUTOMATIC_AUDIO: AudioTrackSelection = { original: false, languages: [] };

export function singleAudioTrack(tracks: AudioTrackSelection): AudioTrackSelection {
  if (tracks.original) return { original: true, languages: [] };
  return { original: false, languages: tracks.languages.slice(0, 1) };
}

export function optionsFromSettings(settings: Settings): DownloadOptions {
  return {
    mediaType: settings.defaultMediaType,
    qualityMode: settings.defaultQualityMode === "exact" || settings.defaultQualityMode === "custom" ? "best" : settings.defaultQualityMode,
    targetHeight: null,
    targetFps: null,
    customFormat: null,
    container: settings.defaultContainer,
    videoCodec: settings.videoCodec,
    audioCodec: settings.audioCodec,
    audioTracks: AUTOMATIC_AUDIO,
    allowHdr: settings.allowHdr,
    audioFormat: settings.defaultAudioFormat,
    audioBitrate: settings.defaultAudioBitrate,
    embedMetadata: settings.embedMetadata,
    embedThumbnail: settings.embedThumbnail,
    writeThumbnail: settings.writeThumbnail,
    embedChapters: settings.embedChapters,
    downloadSubtitles: settings.downloadSubtitles,
    embedSubtitles: settings.embedSubtitles,
    subtitleLanguages: settings.subtitleLanguages,
    includeAutoSubtitles: settings.includeAutoSubtitles,
    filenameTemplate: null,
  };
}

export function qualityModes(): { value: QualityMode; label: string; description: string }[] {
  return [
    { value: "best", label: t("Best available"), description: t("Highest resolution, frame rate and dynamic range the source offers.") },
    { value: "exact", label: t("Exact resolution"), description: t("A specific resolution chosen from the formats the source offers.") },
    { value: "compatible", label: t("Best compatible"), description: t("H.264 + AAC in MP4 for the widest device support.") },
    { value: "custom", label: t("Custom format"), description: t("Enter a yt-dlp format expression such as 137+140.") },
  ];
}

export function containers(): { value: Container; label: string; description: string }[] {
  return [
    { value: "auto", label: t("Auto"), description: t("Keep the streams in their natural container.") },
    { value: "mp4", label: "MP4", description: t("Broad compatibility.") },
    { value: "mkv", label: "MKV", description: t("Holds any codec combination.") },
    { value: "webm", label: "WebM", description: t("VP9/AV1 with Opus only.") },
  ];
}

export function audioFormats(): { value: AudioFormat; label: string; lossless?: boolean }[] {
  return [
    { value: "original", label: t("Original") },
    { value: "mp3", label: "MP3" },
    { value: "m4a", label: "M4A" },
    { value: "aac", label: "AAC" },
    { value: "opus", label: "Opus" },
    { value: "ogg", label: "OGG Vorbis" },
    { value: "flac", label: "FLAC", lossless: true },
    { value: "wav", label: "WAV", lossless: true },
  ];
}

export const AUDIO_BITRATES = [320, 256, 192, 128, 96] as const;

export function stateLabel(state: JobState): string {
  switch (state) {
    case "pending":
      return t("Paused");
    case "analyzing":
      return t("Analyzing");
    case "queued":
      return t("Queued");
    case "downloading":
      return t("Downloading");
    case "merging":
      return t("Merging");
    case "converting":
      return t("Converting");
    case "completed":
      return t("Completed");
    case "failed":
      return t("Failed");
    case "cancelled":
      return t("Cancelled");
  }
}

export function isRunning(state: JobState): boolean {
  return state === "analyzing" || state === "downloading" || state === "merging" || state === "converting";
}

export function isFinished(state: JobState): boolean {
  return state === "completed" || state === "failed" || state === "cancelled";
}

/** Short guidance for errors the user can act on. */
export function errorHint(code: ErrorCode): string | null {
  switch (code) {
    case "dependency_missing":
    case "dependency_failed":
      return t("Open Settings → Dependencies to install or repair components.");
    case "network":
      return t("Check your internet connection, then retry.");
    case "rate_limited":
      return t("Wait a few minutes before retrying.");
    case "disk_full":
      return t("Free up disk space or choose another download folder.");
    case "permission_denied":
    case "invalid_path":
      return t("Choose a download folder you can write to.");
    case "format_unavailable":
      return t("Analyze the link again and pick another quality.");
    default:
      return null;
  }
}

/** Spotify links only ever produce audio, whatever the current media type is. */
export function optionsForPlatform(options: DownloadOptions, platform: string | undefined): DownloadOptions {
  return platform === "spotify" && options.mediaType !== "audio"
    ? { ...options, mediaType: "audio", audioTracks: singleAudioTrack(options.audioTracks ?? AUTOMATIC_AUDIO) }
    : options;
}
