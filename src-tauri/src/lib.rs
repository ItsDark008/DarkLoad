//! DarkLoad desktop backend.

pub mod commands;
pub mod converter;
pub mod dependencies;
pub mod download;
pub mod errors;
pub mod events;
pub mod extractor;
pub mod formats;
pub mod platform;
pub mod process;
pub mod queue;
pub mod security;
pub mod settings;
mod spotify;
pub mod storage;
pub mod thumbnails;
mod tiktok;
pub mod updater;
mod youtube_browser;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use tauri::{AppHandle, Manager, RunEvent};
use tokio::sync::Semaphore;

use commands::{AppPaths, AppState};
use dependencies::DependencyManager;
use queue::{AnalysisCache, QueueManager, QueuePaths};
use settings::Settings;
use storage::Storage;

/// The default download folder: `<Downloads>/DarkLoad`, created on demand.
pub fn default_download_dir(app: &AppHandle) -> String {
    let base = app
        .path()
        .download_dir()
        .or_else(|_| app.path().home_dir())
        .unwrap_or_else(|_| std::env::temp_dir());
    let dir = base.join("DarkLoad");
    let _ = std::fs::create_dir_all(&dir);
    dir.to_string_lossy().into_owned()
}

fn init_logging(log_file: &std::path::Path) {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    if let Some(parent) = log_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Keep the log bounded: start fresh when it grows beyond 5 MB.
    if std::fs::metadata(log_file)
        .map(|m| m.len() > 5 * 1024 * 1024)
        .unwrap_or(false)
    {
        let _ = std::fs::remove_file(log_file);
    }
    let filter = EnvFilter::try_from_env("DARKLOAD_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,darkload_lib=info"));
    let registry = tracing_subscriber::registry().with(filter);
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)
    {
        Ok(file) => {
            let file_layer = fmt::layer()
                .with_ansi(false)
                .with_target(false)
                .with_writer(Mutex::new(file));
            if cfg!(debug_assertions) {
                let _ = registry
                    .with(file_layer)
                    .with(fmt::layer().with_target(false))
                    .try_init();
            } else {
                let _ = registry.with(file_layer).try_init();
            }
        }
        Err(_) => {
            let _ = registry.with(fmt::layer().with_target(false)).try_init();
        }
    }
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let path = app.path();
    let data_dir = path.app_local_data_dir()?;
    let cache_dir = path.app_cache_dir()?;
    let log_file = path.app_log_dir()?.join("darkload.log");
    init_logging(&log_file);
    tracing::info!("starting DarkLoad {}", app.package_info().version);

    if updater::is_configured(&handle) {
        handle.plugin(tauri_plugin_updater::Builder::new().build())?;
    }

    let storage = Arc::new(Storage::open(&data_dir.join("darkload.db"))?);
    let mut settings = storage.load_settings()?.unwrap_or_default();
    if settings.download_dir.trim().is_empty()
        || security::validate_output_dir(&settings.download_dir).is_err()
    {
        settings.download_dir = default_download_dir(&handle);
        storage.save_settings(&settings)?;
    }
    if settings.validate().is_err() {
        tracing::warn!("stored settings were invalid; restoring defaults");
        let dir = settings.download_dir.clone();
        settings = Settings {
            download_dir: dir,
            ..Settings::default()
        };
        storage.save_settings(&settings)?;
    }

    let deps = Arc::new(DependencyManager::new(
        data_dir.join("bin"),
        settings.allow_system_binaries,
    )?);
    let settings = Arc::new(RwLock::new(settings));
    let cache = Arc::new(AnalysisCache::default());
    let paths = AppPaths {
        thumbnails_dir: cache_dir.join("thumbnails"),
        jobs_dir: cache_dir.join("jobs"),
        ytdlp_cache: cache_dir.join("yt-dlp"),
        data_dir,
        log_file,
    };
    let _ = std::fs::create_dir_all(&paths.thumbnails_dir);
    let queue = QueueManager::new(
        storage.clone(),
        deps.clone(),
        settings.clone(),
        cache.clone(),
        QueuePaths {
            jobs_dir: paths.jobs_dir.clone(),
            ytdlp_cache: paths.ytdlp_cache.clone(),
            thumbnails_dir: paths.thumbnails_dir.clone(),
        },
        Arc::new(events::TauriSink {
            app: handle.clone(),
        }),
    );
    let tiktok_guest = Arc::new(tiktok::GuestPlayer::new(handle.clone())?);
    queue.set_guest_player(tiktok_guest.clone());
    let spotify = Arc::new(spotify::SpotifyClient::new()?);
    queue.set_spotify_client(spotify.clone());
    let browser = Arc::new(youtube_browser::YoutubeBrowser::new(handle.clone()));
    let updates = Arc::new(updater::UpdateService::new());
    updates.start(handle.clone(), settings.clone());

    app.manage(AppState {
        storage,
        settings,
        deps: deps.clone(),
        queue: queue.clone(),
        cache,
        paths,
        analyses: Mutex::new(HashMap::new()),
        thumbnail_slots: Semaphore::new(6),
        tiktok_guest,
        spotify,
        browser,
        updates,
    });

    let main_window = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == "main")
        .ok_or_else(|| {
            errors::AppError::internal("The main window is missing from the configuration.")
        })?;
    // The isolated TikTok player needs the browser's normal JavaScript behavior.
    // Apply prototype hardening only to the main window, before its scripts run.
    tauri::WebviewWindowBuilder::from_config(&handle, main_window)?
        .initialization_script("Object.freeze(Object.prototype);")
        .build()?;

    // Detect tools before restoring the queue so restored jobs see accurate dependency state.
    tauri::async_runtime::spawn(async move {
        deps.refresh().await;
        queue.start();
    });
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_filter(|label| !tiktok::is_guest_window(label))
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_process::init())
        .setup(setup)
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::app_check_update,
            commands::app_install_update,
            commands::detect_url,
            commands::analyze_url,
            commands::cancel_analysis,
            commands::cache_thumbnail,
            commands::plan_download,
            commands::enqueue_downloads,
            commands::queue_list,
            commands::queue_cancel,
            commands::queue_pause,
            commands::queue_resume,
            commands::queue_retry,
            commands::queue_remove,
            commands::queue_clear_finished,
            commands::queue_move,
            commands::queue_pause_all,
            commands::queue_resume_all,
            commands::queue_reveal,
            commands::queue_open,
            commands::history_query,
            commands::history_stats,
            commands::history_find_completed,
            commands::history_open,
            commands::history_reveal,
            commands::history_redownload,
            commands::history_delete,
            commands::history_clear,
            commands::open_download_folder,
            commands::settings_get,
            commands::settings_update,
            commands::settings_reset,
            commands::deps_status,
            commands::deps_refresh,
            commands::deps_check_updates,
            commands::deps_install,
            commands::deps_diagnostics,
            commands::clear_temporary_files,
            commands::validate_directory,
            commands::has_active_downloads,
            commands::browser_show,
            commands::browser_hide,
            commands::browser_set_bounds,
            commands::browser_navigate,
            commands::browser_back,
            commands::browser_forward,
            commands::browser_reload,
            commands::browser_sync_selection,
            commands::browser_open_blocked,
        ])
        .build(tauri::generate_context!())
        .expect("error while building DarkLoad");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            if let Some(state) = handle.try_state::<AppState>() {
                let queue = state.queue.clone();
                tauri::async_runtime::block_on(async move { queue.shutdown().await });
                let mode = state
                    .settings
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .update_mode;
                if mode == updater::UpdateMode::Auto {
                    // A downloaded update is installed as DarkLoad closes; it does not reopen.
                    state.updates.install_on_exit();
                }
            }
        }
    });
}
