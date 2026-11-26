import type { AudioFormat, Container, DownloadOptions, ErrorCode, JobState, QualityMode, Settings } from "@/types/api";

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

export const QUALITY_MODES: { value: QualityMode; label: string; description: string }[] = [
  { value: "best", label: "Best available", description: "Highest resolution, frame rate and dynamic range the source offers." },
  { value: "exact", label: "Exact resolution", description: "A specific resolution chosen from the formats the source offers." },
  { value: "compatible", label: "Best compatible", description: "H.264 + AAC in MP4 for the widest device support." },
  { value: "custom", label: "Custom format", description: "Enter a yt-dlp format expression such as 137+140." },
];

export const CONTAINERS: { value: Container; label: string; description: string }[] = [
  { value: "auto", label: "Auto", description: "Keep the streams in their natural container." },
  { value: "mp4", label: "MP4", description: "Broad compatibility." },
  { value: "mkv", label: "MKV", description: "Holds any codec combination." },
  { value: "webm", label: "WebM", description: "VP9/AV1 with Opus only." },
];

export const AUDIO_FORMATS: { value: AudioFormat; label: string; lossless?: boolean }[] = [
  { value: "original", label: "Original" },
  { value: "mp3", label: "MP3" },
  { value: "m4a", label: "M4A" },
  { value: "aac", label: "AAC" },
  { value: "opus", label: "Opus" },
  { value: "ogg", label: "OGG Vorbis" },
  { value: "flac", label: "FLAC", lossless: true },
  { value: "wav", label: "WAV", lossless: true },
];

export const AUDIO_BITRATES = [320, 256, 192, 128, 96] as const;

export const STATE_LABELS: Record<JobState, string> = {
  pending: "Paused",
  analyzing: "Analyzing",
  queued: "Queued",
  downloading: "Downloading",
  merging: "Merging",
  converting: "Converting",
  completed: "Completed",
  failed: "Failed",
  cancelled: "Cancelled",
};

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
      return "Open Settings → Dependencies to install or repair components.";
    case "network":
      return "Check your internet connection, then retry.";
    case "rate_limited":
      return "Wait a few minutes before retrying.";
    case "disk_full":
      return "Free up disk space or choose another download folder.";
    case "permission_denied":
    case "invalid_path":
      return "Choose a download folder you can write to.";
    case "format_unavailable":
      return "Analyze the link again and pick another quality.";
    default:
      return null;
  }
}

/** Spotify links only ever produce audio, whatever the current media type is. */
export function optionsForPlatform(options: DownloadOptions, platform: string | undefined): DownloadOptions {
  return platform === "spotify" && options.mediaType !== "audio" ? { ...options, mediaType: "audio" } : options;
}
