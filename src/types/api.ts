// Types mirroring the Rust backend's serialized structures (serde camelCase).

export type Platform = "youtube" | "tiktok" | "instagram" | "spotify";
export type UrlKind = "video" | "short" | "live" | "playlist" | "reel" | "post" | "track" | "album" | "shortlink";

export interface DetectedUrl {
  platform: Platform;
  kind: UrlKind;
  url: string;
  id: string | null;
  playlistId: string | null;
}

export type ErrorCode =
  | "invalid_url"
  | "unsupported_url"
  | "unavailable"
  | "private_content"
  | "login_required"
  | "age_restricted"
  | "geo_restricted"
  | "drm_protected"
  | "live_not_finished"
  | "format_unavailable"
  | "rate_limited"
  | "network"
  | "disk_full"
  | "permission_denied"
  | "invalid_path"
  | "invalid_input"
  | "dependency_missing"
  | "dependency_failed"
  | "checksum_mismatch"
  | "process_failed"
  | "cancelled"
  | "database"
  | "io"
  | "update_unavailable"
  | "internal";

export interface AppError {
  code: ErrorCode;
  message: string;
  detail: string | null;
  retryable: boolean;
}

export type VideoCodec = "h264" | "h265" | "vp9" | "av1" | "vp8" | "other";
export type AudioCodec = "aac" | "opus" | "vorbis" | "mp3" | "flac" | "ac3" | "other";

export interface MediaFormat {
  id: string;
  ext: string;
  vcodec: string | null;
  acodec: string | null;
  width: number | null;
  height: number | null;
  fps: number | null;
  tbr: number | null;
  vbr: number | null;
  abr: number | null;
  asr: number | null;
  audioChannels: number | null;
  filesize: number | null;
  filesizeApprox: number | null;
  dynamicRange: string | null;
  protocol: string | null;
  note: string | null;
  language: string | null;
  languagePreference: number | null;
}

export interface ResolutionOption {
  height: number;
  label: string;
  frameRates: number[];
  hdr: boolean;
  codecs: VideoCodec[];
}

export interface FormatSummary {
  resolutions: ResolutionOption[];
  videoCodecs: VideoCodec[];
  audioCodecs: AudioCodec[];
  audioBitrates: number[];
  hasVideo: boolean;
  hasAudio: boolean;
  hasHdr: boolean;
  maxHeight: number | null;
  maxFps: number | null;
}

export interface SubtitleTrack {
  language: string;
  name: string | null;
  automatic: boolean;
}

/** Song metadata from Spotify and the YouTube upload matched as its audio source. */
export interface SpotifyTrack {
  id: string;
  title: string;
  artists: string[];
  album: string | null;
  albumArtist: string | null;
  trackNumber: number | null;
  releaseDate: string | null;
  duration: number | null;
  cover: string | null;
  isrc: string | null;
  explicit: boolean;
  playable: boolean;
  sourceUrl: string | null;
  sourceTitle: string | null;
  sourceChannel: string | null;
}

export interface MediaInfo {
  id: string;
  url: string;
  title: string;
  uploader: string | null;
  duration: number | null;
  thumbnail: string | null;
  thumbnailLocal: string | null;
  platform: Platform;
  wasLive: boolean;
  uploadDate: string | null;
  viewCount: number | null;
  chapters: number;
  formats: MediaFormat[];
  summary: FormatSummary;
  subtitles: SubtitleTrack[];
  playlistId: string | null;
  playlistItem: number | null;
  spotify?: SpotifyTrack | null;
}

export interface PlaylistEntry {
  id: string;
  url: string;
  title: string;
  uploader: string | null;
  duration: number | null;
  thumbnail: string | null;
  index: number;
  playlistItem: number | null;
  available: boolean;
  media: MediaInfo | null;
  spotify?: SpotifyTrack | null;
}

export interface PlaylistInfo {
  id: string;
  url: string;
  title: string;
  uploader: string | null;
  platform: Platform;
  thumbnail: string | null;
  entries: PlaylistEntry[];
  notice?: string | null;
}

export type Analysis = ({ kind: "media" } & MediaInfo) | ({ kind: "playlist" } & PlaylistInfo);

export type MediaType = "video" | "audio";
export type QualityMode = "best" | "exact" | "compatible" | "custom";
export type Container = "auto" | "mp4" | "mkv" | "webm";
export type VideoCodecPreference = "any" | "av1" | "vp9" | "h265" | "h264";
export type AudioCodecPreference = "any" | "opus" | "aac";
export type AudioFormat = "original" | "mp3" | "m4a" | "aac" | "flac" | "wav" | "opus" | "ogg";

export interface AudioTrackSelection {
  original: boolean;
  languages: string[];
}

export interface DownloadOptions {
  mediaType: MediaType;
  qualityMode: QualityMode;
  targetHeight: number | null;
  targetFps: number | null;
  customFormat: string | null;
  container: Container;
  videoCodec: VideoCodecPreference;
  audioCodec: AudioCodecPreference;
  audioTracks: AudioTrackSelection;
  allowHdr: boolean;
  audioFormat: AudioFormat;
  audioBitrate: number | null;
  embedMetadata: boolean;
  embedThumbnail: boolean;
  writeThumbnail: boolean;
  embedChapters: boolean;
  downloadSubtitles: boolean;
  embedSubtitles: boolean;
  subtitleLanguages: string[];
  includeAutoSubtitles: boolean;
  filenameTemplate: string | null;
}

export interface DownloadPreset {
  name: string;
  options: DownloadOptions;
  outputDir: string | null;
}

export interface PlanNote {
  level: "info" | "warning";
  message: string;
}

export interface ContainerConflict {
  requested: Container;
  suggested: Container;
  reason: string;
}

export interface SelectionPlan {
  video: MediaFormat | null;
  audio: MediaFormat | null;
  additionalAudio: MediaFormat[];
  formatSelector: string;
  formatSort: string | null;
  outputContainer: Container | null;
  requiresMerge: boolean;
  audioConversion: AudioFormat | null;
  transcodesAudio: boolean;
  estimatedSize: number | null;
  sizeIsEstimate: boolean;
  qualityLabel: string;
  notes: PlanNote[];
  containerConflict: ContainerConflict | null;
  embedThumbnail: boolean;
  embedSubtitles: boolean;
}

export interface DownloadRequest {
  url: string;
  outputDir?: string | null;
  options: DownloadOptions;
  playlistItem?: number | null;
  title?: string | null;
  uploader?: string | null;
  thumbnail?: string | null;
  thumbnailLocal?: string | null;
  duration?: number | null;
  spotify?: SpotifyTrack | null;
}

export type JobState =
  | "pending"
  | "analyzing"
  | "queued"
  | "downloading"
  | "merging"
  | "converting"
  | "completed"
  | "failed"
  | "cancelled";

export interface JobProgress {
  downloadFraction: number | null;
  downloadedBytes: number;
  totalBytes: number | null;
  speed: number | null;
  eta: number | null;
  processLabel: string | null;
  processFraction: number | null;
  processEta: number | null;
  retryLabel: string | null;
}

export interface Job {
  id: string;
  request: DownloadRequest;
  platform: Platform;
  state: JobState;
  title: string;
  uploader: string | null;
  thumbnail: string | null;
  thumbnailLocal: string | null;
  duration: number | null;
  outputDir: string;
  plan: SelectionPlan | null;
  progress: JobProgress;
  error: AppError | null;
  notes: string[];
  attempts: number;
  paused: boolean;
  createdAt: number;
  startedAt: number | null;
  finishedAt: number | null;
  outputPath: string | null;
  fileSize: number | null;
}

export interface HistoryEntry {
  id: string;
  url: string;
  title: string;
  uploader: string | null;
  duration: number | null;
  thumbnail: string | null;
  thumbnailLocal: string | null;
  platform: Platform;
  mediaType: MediaType;
  format: string | null;
  quality: string | null;
  fileSize: number | null;
  filePath: string | null;
  status: "completed" | "failed" | "cancelled";
  error: string | null;
  createdAt: number;
  completedAt: number | null;
  fileExists: boolean;
}

export type HistorySort = "newest" | "oldest" | "title" | "size";

export interface HistoryQuery {
  search?: string | null;
  platform?: string | null;
  mediaType?: string | null;
  status?: string | null;
  sort?: HistorySort;
  limit?: number;
  offset?: number;
}

export interface HistoryPage {
  items: HistoryEntry[];
  total: number;
}

export interface HistoryStats {
  completed: number;
  failed: number;
  cancelled: number;
  totalBytes: number;
  byPlatform: { platform: Platform; count: number }[];
  recent: HistoryEntry[];
}

export type DuplicatePolicy = "rename" | "overwrite" | "skip";

export interface Settings {
  downloadDir: string;
  defaultMediaType: MediaType;
  defaultContainer: Container;
  defaultQualityMode: QualityMode;
  defaultAudioFormat: AudioFormat;
  defaultAudioBitrate: number | null;
  filenameTemplate: string;
  nativeWindowFrame: boolean;
  confirmExitWithActiveDownloads: boolean;
  maxConcurrentDownloads: number;
  speedLimitKib: number | null;
  retryAttempts: number;
  socketTimeoutSecs: number;
  concurrentFragments: number;
  duplicatePolicy: DuplicatePolicy;
  partialFiles: "delete" | "keep";
  playlistBehavior: "select" | "all";
  resumeQueueOnStartup: boolean;
  downloadPresets: DownloadPreset[];
  videoCodec: VideoCodecPreference;
  audioCodec: AudioCodecPreference;
  allowHdr: boolean;
  embedMetadata: boolean;
  embedThumbnail: boolean;
  writeThumbnail: boolean;
  embedChapters: boolean;
  downloadSubtitles: boolean;
  embedSubtitles: boolean;
  subtitleLanguages: string[];
  includeAutoSubtitles: boolean;
  theme: "oled" | "dark";
  accentColor: string;
  oledBackground: string;
  sidebarCollapsed: boolean;
  sidebarBehavior: "remember" | "auto";
  density: "comfortable" | "compact";
  animations: boolean;
  updateMode: UpdateMode;
  clipboardMonitoring: boolean;
  notifications: boolean;
  allowSystemBinaries: boolean;
  preferIpv4: boolean;
  spotifyClientId: string;
  spotifyClientSecret: string;
}

export type DepId = "yt-dlp" | "ffmpeg" | "ffprobe" | "deno";

export interface DependencyStatus {
  id: DepId;
  name: string;
  description: string;
  required: boolean;
  available: boolean;
  version: string | null;
  path: string | null;
  source: "managed" | "system" | "missing";
  installedAt: number | null;
  error: string | null;
  latestVersion: string | null;
  updateAvailable: boolean | null;
}

export interface InstallProgress {
  id: DepId;
  stage: "resolving" | "downloading" | "verifying" | "extracting" | "validating" | "done" | "failed";
  downloaded: number;
  total: number | null;
  message: string | null;
}

export interface AppInfo {
  name: string;
  version: string;
  tauriVersion: string;
  os: string;
  arch: string;
  dataDir: string;
  logFile: string;
  updaterConfigured: boolean;
  repository: string | null;
  changelog: string;
}

export type UpdateMode = "auto" | "notify" | "off";

export interface UpdateInfo {
  available: boolean;
  currentVersion: string;
  version: string | null;
  notes: string | null;
  date: string | null;
  /** A check has completed since DarkLoad started. */
  checked: boolean;
  /** The new version is downloaded, verified and ready to install. */
  ready: boolean;
  /** This copy was installed with an installer that can update it in place (not portable). */
  installable: boolean;
}

export interface DeleteResult {
  entries: number;
  files: number;
  fileErrors: number;
}

// ----- YouTube browser --------------------------------------------------------------------------

/** Where the browser sits in the main window, in CSS pixels. */
export interface BrowserBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** A video picked in the built-in browser. The URL and thumbnail are rebuilt by the backend. */
export interface BrowserVideo {
  id: string;
  url: string;
  title: string | null;
  uploader: string | null;
  duration: number | null;
  thumbnail: string;
}

export type BrowserEvent =
  | { kind: "toggle"; video: BrowserVideo; selected: boolean }
  | { kind: "page"; url: string; title: string | null; video: BrowserVideo | null }
  | { kind: "loading"; loading: boolean }
  | { kind: "blocked"; url: string };
