//! Typed IPC commands. Each command validates its input and delegates to a backend module.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::dependencies::{DepId, DependencyManager, DependencyStatus};
use crate::errors::{AppError, AppResult, ErrorCode};
use crate::events;
use crate::extractor::{self, Analysis};
use crate::formats::{self, DownloadOptions, SelectionPlan};
use crate::platform::{self, DetectedUrl};
use crate::queue::{AnalysisCache, DownloadRequest, Job, QueueManager};
use crate::settings::Settings;
use crate::storage::{HistoryEntry, HistoryPage, HistoryQuery, HistoryStats, Storage};
use crate::updater::{self, UpdateInfo};
use crate::{accounts, security, thumbnails};

pub struct AppPaths {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub thumbnails_dir: PathBuf,
    pub jobs_dir: PathBuf,
    pub ytdlp_cache: PathBuf,
    pub log_file: PathBuf,
}

pub struct AppState {
    pub storage: Arc<Storage>,
    pub settings: Arc<RwLock<Settings>>,
    pub deps: Arc<DependencyManager>,
    pub queue: Arc<QueueManager>,
    pub cache: Arc<AnalysisCache>,
    pub paths: AppPaths,
    pub analyses: Mutex<HashMap<String, CancellationToken>>,
    pub thumbnail_slots: Semaphore,
    pub(crate) tiktok_guest: Arc<crate::tiktok::GuestPlayer>,
}

impl AppState {
    fn settings(&self) -> Settings {
        self.settings
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

type Cmd<T> = Result<T, AppError>;

// ----- Application ----------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    name: String,
    version: String,
    tauri_version: String,
    os: String,
    arch: String,
    data_dir: String,
    log_file: String,
    updater_configured: bool,
    repository: Option<String>,
    changelog: String,
}

#[tauri::command]
pub fn app_info(app: AppHandle, state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        name: app.package_info().name.clone(),
        version: app.package_info().version.to_string(),
        tauri_version: tauri::VERSION.to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        data_dir: state.paths.data_dir.to_string_lossy().into_owned(),
        log_file: state.paths.log_file.to_string_lossy().into_owned(),
        updater_configured: updater::is_configured(&app),
        repository: option_env!("DARKLOAD_REPOSITORY")
            .map(str::to_string)
            .filter(|s| !s.is_empty()),
        changelog: include_str!("../../CHANGELOG.md").to_string(),
    }
}

#[tauri::command]
pub async fn app_check_update(app: AppHandle) -> Cmd<UpdateInfo> {
    updater::check(&app).await
}

#[tauri::command]
pub async fn app_install_update(app: AppHandle, state: State<'_, AppState>) -> Cmd<()> {
    if state.queue.running_count() > 0 {
        return Err(AppError::invalid_input(
            "Finish or pause active downloads before installing the update.",
        ));
    }
    updater::install(&app).await?;
    state.queue.shutdown().await;
    app.restart();
}

// ----- Analysis -------------------------------------------------------------------------------

#[tauri::command]
pub fn detect_url(url: String) -> Cmd<DetectedUrl> {
    platform::detect(&url)
}

#[tauri::command]
pub async fn analyze_url(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    request_id: String,
) -> Cmd<Analysis> {
    let detected = platform::detect(&url)?;
    if request_id.is_empty() || request_id.len() > 64 {
        return Err(AppError::invalid_input("Invalid analysis request."));
    }
    let tools = state
        .deps
        .tool_paths(state.paths.ytdlp_cache.clone())
        .await?;
    let token = CancellationToken::new();
    state
        .analyses
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(request_id.clone(), token.clone());
    let net = extractor::NetOptions::from(&state.settings());
    // Use the connected account's session, if any, in a temporary file for this analysis only.
    let session_dir = state
        .paths
        .cache_dir
        .join("sessions")
        .join(uuid::Uuid::new_v4().simple().to_string());
    let result = async {
        let cookie_file = match accounts::export(&app, detected.platform).await {
            Some(text) => Some(accounts::write_cookie_file(&session_dir, &text)?),
            None => None,
        };
        match extractor::analyze(&tools, &detected, net, cookie_file.as_deref(), &token).await {
            Err(e)
                if cookie_file.is_none() && crate::tiktok::GuestPlayer::can_try(&detected, &e) =>
            {
                match state.tiktok_guest.resolve(&detected, &token).await {
                    Ok(video) => video.analysis(),
                    Err(guest_error) if guest_error.code == ErrorCode::Cancelled => {
                        Err(guest_error)
                    }
                    Err(guest_error) => Err(accounts::login_hint(detected.platform)
                        .unwrap_or(e)
                        .with_detail(guest_error.message)),
                }
            }
            Err(e) if e.code == ErrorCode::LoginRequired && cookie_file.is_none() => {
                Err(accounts::login_hint(detected.platform)
                    .map(|hint| hint.with_detail(e.detail.clone().unwrap_or_default()))
                    .unwrap_or(e))
            }
            other => other,
        }
    }
    .await;
    let _ = std::fs::remove_dir_all(&session_dir);
    state
        .analyses
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&request_id);
    let mut analysis = result?;

    match &mut analysis {
        Analysis::Media(media) => {
            if let Some(url) = media.thumbnail.clone() {
                media.thumbnail_local =
                    thumbnails::cache(state.deps.http(), &state.paths.thumbnails_dir, &url)
                        .await
                        .map(path_string);
            }
            // Cache under the canonical form the queue and planner use for lookups. Short links
            // resolve to the extractor's page URL; anything unrecognized keeps the input URL.
            media.url = platform::detect(&media.url)
                .map(|d| d.url)
                .unwrap_or_else(|_| detected.url.clone());
            state
                .cache
                .put(&media.url, media.playlist_item, (**media).clone());
            if media.url != detected.url {
                state
                    .cache
                    .put(&detected.url, media.playlist_item, (**media).clone());
            }
        }
        Analysis::Playlist(list) => {
            for entry in &mut list.entries {
                if let Ok(d) = platform::detect(&entry.url) {
                    entry.url = d.url;
                }
                if let Some(media) = entry.media.as_mut() {
                    media.url = entry.url.clone();
                    state
                        .cache
                        .put(&entry.url, entry.playlist_item, (**media).clone());
                }
            }
        }
    }
    Ok(analysis)
}

#[tauri::command]
pub fn cancel_analysis(state: State<'_, AppState>, request_id: String) {
    if let Some(token) = state
        .analyses
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&request_id)
    {
        token.cancel();
    }
}

#[tauri::command]
pub async fn cache_thumbnail(state: State<'_, AppState>, url: String) -> Cmd<Option<String>> {
    let _permit = state
        .thumbnail_slots
        .acquire()
        .await
        .map_err(|_| AppError::internal("Thumbnail cache unavailable."))?;
    Ok(
        thumbnails::cache(state.deps.http(), &state.paths.thumbnails_dir, &url)
            .await
            .map(path_string),
    )
}

#[tauri::command]
pub fn plan_download(
    state: State<'_, AppState>,
    url: String,
    playlist_item: Option<u32>,
    options: DownloadOptions,
) -> Cmd<SelectionPlan> {
    let detected = platform::detect(&url)?;
    let media = state
        .cache
        .get(&detected.url, playlist_item)
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::InvalidInput,
                "Analyze this link before choosing a quality.",
            )
        })?;
    formats::plan(&media.formats, media.duration, &options)
}

// ----- Queue ----------------------------------------------------------------------------------

#[tauri::command]
pub fn enqueue_downloads(
    state: State<'_, AppState>,
    requests: Vec<DownloadRequest>,
) -> Cmd<Vec<Job>> {
    state.queue.enqueue(requests)
}

#[tauri::command]
pub fn queue_list(state: State<'_, AppState>) -> Vec<Job> {
    state.queue.list()
}

#[tauri::command]
pub fn queue_cancel(state: State<'_, AppState>, id: String) -> Cmd<()> {
    state.queue.cancel(&id)
}

#[tauri::command]
pub fn queue_pause(state: State<'_, AppState>, id: String) -> Cmd<()> {
    state.queue.pause(&id)
}

#[tauri::command]
pub fn queue_resume(state: State<'_, AppState>, id: String) -> Cmd<()> {
    state.queue.resume(&id)
}

#[tauri::command]
pub fn queue_retry(state: State<'_, AppState>, id: String) -> Cmd<()> {
    state.queue.retry(&id)
}

#[tauri::command]
pub fn queue_remove(state: State<'_, AppState>, ids: Vec<String>) -> Cmd<()> {
    state.queue.remove(&ids)
}

#[tauri::command]
pub fn queue_clear_finished(state: State<'_, AppState>) -> usize {
    state.queue.clear_finished()
}

#[tauri::command]
pub fn queue_move(state: State<'_, AppState>, id: String, index: usize) -> Cmd<()> {
    state.queue.move_to(&id, index)
}

#[tauri::command]
pub fn queue_pause_all(state: State<'_, AppState>) {
    state.queue.pause_all();
}

#[tauri::command]
pub fn queue_resume_all(state: State<'_, AppState>) {
    state.queue.resume_all();
}

// ----- History --------------------------------------------------------------------------------

#[tauri::command]
pub fn history_query(state: State<'_, AppState>, query: HistoryQuery) -> Cmd<HistoryPage> {
    state.storage.query_history(&query)
}

#[tauri::command]
pub fn history_stats(state: State<'_, AppState>) -> Cmd<HistoryStats> {
    state.storage.stats()
}

#[tauri::command]
pub fn history_find_completed(
    state: State<'_, AppState>,
    url: String,
) -> Cmd<Option<HistoryEntry>> {
    let detected = platform::detect(&url)?;
    state.storage.find_completed(&detected.url)
}

fn history_file(state: &AppState, id: &str) -> AppResult<PathBuf> {
    let entry = state
        .storage
        .get_history(id)?
        .ok_or_else(|| AppError::invalid_input("This history entry no longer exists."))?;
    let path = entry
        .file_path
        .map(PathBuf::from)
        .ok_or_else(|| AppError::new(ErrorCode::InvalidPath, "This download has no file."))?;
    if !path.is_file() {
        return Err(AppError::new(
            ErrorCode::InvalidPath,
            "The file was moved or deleted.",
        ));
    }
    Ok(path)
}

#[tauri::command]
pub fn history_open(app: AppHandle, state: State<'_, AppState>, id: String) -> Cmd<()> {
    let path = history_file(&state, &id)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| {
            AppError::new(ErrorCode::Io, "The file could not be opened.").with_detail(e.to_string())
        })
}

#[tauri::command]
pub fn history_reveal(app: AppHandle, state: State<'_, AppState>, id: String) -> Cmd<()> {
    let path = history_file(&state, &id)?;
    app.opener().reveal_item_in_dir(&path).map_err(|e| {
        AppError::new(ErrorCode::Io, "The folder could not be opened.").with_detail(e.to_string())
    })
}

#[tauri::command]
pub fn open_download_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    path: Option<String>,
) -> Cmd<()> {
    let dir = path.unwrap_or_else(|| state.settings().download_dir);
    let dir = security::validate_output_dir(&dir)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| {
            AppError::new(ErrorCode::Io, "The folder could not be opened.")
                .with_detail(e.to_string())
        })
}

#[tauri::command]
pub fn queue_reveal(app: AppHandle, state: State<'_, AppState>, id: String) -> Cmd<()> {
    let job = state
        .queue
        .list()
        .into_iter()
        .find(|j| j.id == id)
        .ok_or_else(|| AppError::invalid_input("Unknown download."))?;
    let path = job
        .output_path
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .ok_or_else(|| AppError::new(ErrorCode::InvalidPath, "The file was moved or deleted."))?;
    app.opener().reveal_item_in_dir(&path).map_err(|e| {
        AppError::new(ErrorCode::Io, "The folder could not be opened.").with_detail(e.to_string())
    })
}

#[tauri::command]
pub fn queue_open(app: AppHandle, state: State<'_, AppState>, id: String) -> Cmd<()> {
    let job = state
        .queue
        .list()
        .into_iter()
        .find(|j| j.id == id)
        .ok_or_else(|| AppError::invalid_input("Unknown download."))?;
    let path = job
        .output_path
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .ok_or_else(|| AppError::new(ErrorCode::InvalidPath, "The file was moved or deleted."))?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| {
            AppError::new(ErrorCode::Io, "The file could not be opened.").with_detail(e.to_string())
        })
}

#[tauri::command]
pub fn history_redownload(state: State<'_, AppState>, id: String) -> Cmd<Job> {
    let entry = state
        .storage
        .get_history(&id)?
        .ok_or_else(|| AppError::invalid_input("This history entry no longer exists."))?;
    let mut request: DownloadRequest = serde_json::from_str(&entry.request_json).map_err(|_| {
        AppError::invalid_input(
            "This entry was created by an older version and cannot be repeated.",
        )
    })?;
    if request
        .output_dir
        .as_deref()
        .is_some_and(|d| security::validate_output_dir(d).is_err())
    {
        request.output_dir = None;
    }
    state
        .queue
        .enqueue(vec![request])?
        .pop()
        .ok_or_else(|| AppError::internal("Failed to add the download."))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    entries: usize,
    files: usize,
    file_errors: usize,
}

#[tauri::command]
pub fn history_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
    delete_files: bool,
) -> Cmd<DeleteResult> {
    let mut files = 0;
    let mut file_errors = 0;
    if delete_files {
        for id in &ids {
            if let Some(path) = state
                .storage
                .get_history(id)?
                .and_then(|e| e.file_path)
                .map(PathBuf::from)
            {
                if path.is_file() {
                    match std::fs::remove_file(&path) {
                        Ok(()) => files += 1,
                        Err(_) => file_errors += 1,
                    }
                }
            }
        }
    }
    let entries = state.storage.delete_history(&ids)?;
    let _ = app.emit(events::HISTORY_CHANGED, ());
    Ok(DeleteResult {
        entries,
        files,
        file_errors,
    })
}

#[tauri::command]
pub fn history_clear(app: AppHandle, state: State<'_, AppState>) -> Cmd<usize> {
    let removed = state.storage.clear_history()?;
    let _ = thumbnails::clear(&state.paths.thumbnails_dir);
    state.cache.clear();
    let _ = app.emit(events::HISTORY_CHANGED, ());
    Ok(removed)
}

// ----- Settings -------------------------------------------------------------------------------

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn settings_update(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Cmd<Settings> {
    settings.validate()?;
    if !settings.download_dir.is_empty() {
        security::validate_output_dir(&settings.download_dir)?;
    }
    state.storage.save_settings(&settings)?;
    let previous = std::mem::replace(
        &mut *state.settings.write().unwrap_or_else(|e| e.into_inner()),
        settings.clone(),
    );
    if previous.allow_system_binaries != settings.allow_system_binaries {
        state.deps.set_allow_system(settings.allow_system_binaries);
        let deps = state.deps.clone();
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let statuses = deps.refresh().await;
            let _ = handle.emit(events::DEPS_CHANGED, statuses);
        });
    }
    state.queue.wake();
    let _ = app.emit(events::SETTINGS_CHANGED, &settings);
    Ok(settings)
}

#[tauri::command]
pub fn settings_reset(app: AppHandle, state: State<'_, AppState>) -> Cmd<Settings> {
    let defaults = Settings {
        download_dir: crate::default_download_dir(&app),
        ..Settings::default()
    };
    settings_update(app, state, defaults)
}

// ----- Dependencies ---------------------------------------------------------------------------

#[tauri::command]
pub async fn deps_status(state: State<'_, AppState>) -> Cmd<Vec<DependencyStatus>> {
    Ok(state.deps.statuses().await)
}

#[tauri::command]
pub async fn deps_refresh(state: State<'_, AppState>) -> Cmd<Vec<DependencyStatus>> {
    Ok(state.deps.refresh().await)
}

#[tauri::command]
pub async fn deps_check_updates(state: State<'_, AppState>) -> Cmd<Vec<DependencyStatus>> {
    state.deps.check_updates().await
}

#[tauri::command]
pub async fn deps_install(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<DepId>,
    repair: bool,
) -> Cmd<Vec<DependencyStatus>> {
    if ids.is_empty() {
        return Err(AppError::invalid_input("Choose at least one component."));
    }
    if state.queue.running_count() > 0 {
        return Err(AppError::invalid_input(
            "Pause or finish active downloads before installing components.",
        ));
    }
    if repair {
        for id in &ids {
            state.deps.remove_managed(*id)?;
        }
    }
    let handle = app.clone();
    let result = state
        .deps
        .install(&ids, move |progress| {
            let _ = handle.emit(events::DEPS_PROGRESS, progress);
        })
        .await;
    let statuses = state.deps.statuses().await;
    let _ = app.emit(events::DEPS_CHANGED, &statuses);
    result
}

#[tauri::command]
pub async fn deps_diagnostics(state: State<'_, AppState>) -> Cmd<String> {
    let mut report = state.deps.diagnostics().await;
    report.push_str(&format!(
        "Data folder: {}\nLog file: {}\n",
        state.paths.data_dir.display(),
        state.paths.log_file.display()
    ));
    if let Ok(log) = std::fs::read_to_string(&state.paths.log_file) {
        let tail: Vec<&str> = log.lines().rev().take(40).collect();
        report.push_str("\nRecent log entries:\n");
        for line in tail.into_iter().rev() {
            report.push_str(line);
            report.push('\n');
        }
    }
    Ok(report)
}

// ----- Privacy & maintenance ------------------------------------------------------------------

#[tauri::command]
pub fn clear_temporary_files(state: State<'_, AppState>) -> Cmd<u64> {
    let freed = state.queue.clear_temporary_files()?;
    state.cache.clear();
    Ok(freed)
}

#[tauri::command]
pub fn validate_directory(path: String) -> Cmd<String> {
    security::validate_output_dir(&path).map(path_string)
}

#[tauri::command]
pub fn has_active_downloads(state: State<'_, AppState>) -> bool {
    state.queue.running_count() > 0
}

fn path_string(p: PathBuf) -> String {
    p.to_string_lossy().into_owned()
}

// ----- Accounts -------------------------------------------------------------------------------

fn account_platform(platform: &str) -> Cmd<platform::Platform> {
    platform::Platform::parse(platform)
        .filter(|p| accounts::supports(*p))
        .ok_or_else(|| {
            AppError::invalid_input("Accounts can be connected for TikTok and Instagram.")
        })
}

#[tauri::command]
pub async fn accounts_status(app: AppHandle) -> Cmd<Vec<accounts::AccountStatus>> {
    Ok(accounts::status(&app).await)
}

#[tauri::command]
pub async fn accounts_connect(app: AppHandle, platform: String) -> Cmd<()> {
    accounts::connect(&app, account_platform(&platform)?).await
}

#[tauri::command]
pub async fn accounts_disconnect(app: AppHandle, platform: String) -> Cmd<()> {
    accounts::disconnect(&app, account_platform(&platform)?).await
}
