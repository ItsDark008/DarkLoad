// Typed bridge to the Rust backend. Every IPC call goes through this module.
import { invoke } from "@tauri-apps/api/core";
import type {
  Analysis,
  AccountStatus,
  AppError,
  AppInfo,
  DeleteResult,
  DependencyStatus,
  DepId,
  DetectedUrl,
  DownloadOptions,
  DownloadRequest,
  HistoryEntry,
  HistoryPage,
  HistoryQuery,
  HistoryStats,
  Platform,
  Job,
  SelectionPlan,
  Settings,
  UpdateInfo,
} from "@/types/api";

/** Normalizes anything thrown by `invoke` into an {@link AppError}. */
export function toAppError(error: unknown): AppError {
  if (error && typeof error === "object" && "code" in error && "message" in error) {
    const e = error as Partial<AppError>;
    return {
      code: e.code ?? "internal",
      message: String(e.message ?? "Something went wrong."),
      detail: e.detail ?? null,
      retryable: Boolean(e.retryable),
    };
  }
  const message = typeof error === "string" ? error : error instanceof Error ? error.message : "Something went wrong.";
  return { code: "internal", message, detail: null, retryable: false };
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toAppError(error);
  }
}

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  checkAppUpdate: () => call<UpdateInfo>("app_check_update"),
  installAppUpdate: () => call<void>("app_install_update"),

  detectUrl: (url: string) => call<DetectedUrl>("detect_url", { url }),
  analyzeUrl: (url: string, requestId: string) => call<Analysis>("analyze_url", { url, requestId }),
  cancelAnalysis: (requestId: string) => call<void>("cancel_analysis", { requestId }),
  cacheThumbnail: (url: string) => call<string | null>("cache_thumbnail", { url }),
  planDownload: (url: string, playlistItem: number | null, options: DownloadOptions) =>
    call<SelectionPlan>("plan_download", { url, playlistItem, options }),

  enqueue: (requests: DownloadRequest[]) => call<Job[]>("enqueue_downloads", { requests }),
  queueList: () => call<Job[]>("queue_list"),
  queueCancel: (id: string) => call<void>("queue_cancel", { id }),
  queuePause: (id: string) => call<void>("queue_pause", { id }),
  queueResume: (id: string) => call<void>("queue_resume", { id }),
  queueRetry: (id: string) => call<void>("queue_retry", { id }),
  queueRemove: (ids: string[]) => call<void>("queue_remove", { ids }),
  queueClearFinished: () => call<number>("queue_clear_finished"),
  queueMove: (id: string, index: number) => call<void>("queue_move", { id, index }),
  queuePauseAll: () => call<void>("queue_pause_all"),
  queueResumeAll: () => call<void>("queue_resume_all"),
  queueReveal: (id: string) => call<void>("queue_reveal", { id }),
  queueOpen: (id: string) => call<void>("queue_open", { id }),

  historyQuery: (query: HistoryQuery) => call<HistoryPage>("history_query", { query }),
  historyStats: () => call<HistoryStats>("history_stats"),
  historyFindCompleted: (url: string) => call<HistoryEntry | null>("history_find_completed", { url }),
  historyOpen: (id: string) => call<void>("history_open", { id }),
  historyReveal: (id: string) => call<void>("history_reveal", { id }),
  historyRedownload: (id: string) => call<Job>("history_redownload", { id }),
  historyDelete: (ids: string[], deleteFiles: boolean) => call<DeleteResult>("history_delete", { ids, deleteFiles }),
  historyClear: () => call<number>("history_clear"),
  openDownloadFolder: (path?: string | null) => call<void>("open_download_folder", { path: path ?? null }),

  settingsGet: () => call<Settings>("settings_get"),
  settingsUpdate: (settings: Settings) => call<Settings>("settings_update", { settings }),
  settingsReset: () => call<Settings>("settings_reset"),

  depsStatus: () => call<DependencyStatus[]>("deps_status"),
  depsRefresh: () => call<DependencyStatus[]>("deps_refresh"),
  depsCheckUpdates: () => call<DependencyStatus[]>("deps_check_updates"),
  depsInstall: (ids: DepId[], repair = false) => call<DependencyStatus[]>("deps_install", { ids, repair }),
  depsDiagnostics: () => call<string>("deps_diagnostics"),

  clearTemporaryFiles: () => call<number>("clear_temporary_files"),
  validateDirectory: (path: string) => call<string>("validate_directory", { path }),
  hasActiveDownloads: () => call<boolean>("has_active_downloads"),

  accountsStatus: () => call<AccountStatus[]>("accounts_status"),
  accountsConnect: (platform: Platform) => call<void>("accounts_connect", { platform }),
  accountsDisconnect: (platform: Platform) => call<void>("accounts_disconnect", { platform }),
};

export const events = {
  queueJob: "queue://job",
  queueChanged: "queue://changed",
  depsProgress: "deps://progress",
  depsChanged: "deps://changed",
  settingsChanged: "settings://changed",
  updaterProgress: "updater://progress",
  historyChanged: "history://changed",
  accountsChanged: "accounts://changed",
} as const;
