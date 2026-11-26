//! Signed application updates through the Tauri updater.
//!
//! The updater is only active in builds that were configured with an update endpoint and a
//! public signing key (see `docs/RELEASING.md`). Other builds report that updates are unavailable
//! instead of pretending to check.
//!
//! [`UpdateService`] checks in the background at startup and every few hours. Depending on the
//! user's update mode it only reports a new version, or also downloads it so it can be installed
//! with one click or automatically when DarkLoad closes. Every package is verified against the
//! public key before it is installed.

use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Notify;

use crate::errors::{AppError, AppResult, ErrorCode};
use crate::events::{UPDATER_PROGRESS, UPDATER_STATUS};
use crate::settings::Settings;

/// Delay before the first background check, so startup work finishes first.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(20);
/// Interval between background checks.
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 3600);
/// Wait before retrying after a failed check or download.
const RETRY_DELAY: Duration = Duration::from_secs(30 * 60);

/// How DarkLoad handles new versions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateMode {
    /// Download new versions in the background and install them on restart or exit.
    #[default]
    Auto,
    /// Only report new versions.
    Notify,
    /// Never check in the background.
    Off,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub available: bool,
    pub current_version: String,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub date: Option<String>,
    /// Whether a check has completed since DarkLoad started.
    pub checked: bool,
    /// The new version is downloaded, verified and ready to install.
    pub ready: bool,
    /// This copy was installed with an installer that can update it in place.
    pub installable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProgress {
    downloaded: u64,
    total: Option<u64>,
    finished: bool,
}

/// Whether this build carries an updater configuration with a public key and endpoints.
pub fn is_configured<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .map(|cfg| {
            let pubkey = cfg
                .get("pubkey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let endpoints = cfg
                .get("endpoints")
                .and_then(|v| v.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            !pubkey.is_empty() && endpoints
        })
        .unwrap_or(false)
}

/// Whether this copy was installed by an installer the updater can run again (NSIS, MSI,
/// AppImage, deb or rpm). Portable copies and development builds report `false`.
pub fn is_installable() -> bool {
    tauri::utils::platform::bundle_type().is_some()
}

fn not_configured() -> AppError {
    AppError::new(
        ErrorCode::UpdateUnavailable,
        "Automatic updates are not configured for this build. Download new versions from the project's releases page.",
    )
}

fn not_installable() -> AppError {
    AppError::new(
        ErrorCode::UpdateUnavailable,
        "This portable copy of DarkLoad cannot update itself. Download the new version from the project's releases page.",
    )
}

#[derive(Default)]
struct State {
    checked: bool,
    update: Option<Update>,
    /// Verified package bytes for `update`.
    package: Option<Vec<u8>>,
    downloading: bool,
}

/// Background update checks, downloads and installation.
pub struct UpdateService {
    state: Mutex<State>,
    wake: Notify,
    /// Serializes network work so manual and background checks never overlap.
    busy: tokio::sync::Mutex<()>,
}

impl Default for UpdateService {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdateService {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State::default()),
            wake: Notify::new(),
            busy: tokio::sync::Mutex::new(()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The last known update status, without contacting the server.
    pub fn status<R: Runtime>(&self, app: &AppHandle<R>) -> UpdateInfo {
        let state = self.lock();
        let update = state.update.as_ref();
        UpdateInfo {
            available: update.is_some(),
            current_version: app.package_info().version.to_string(),
            version: update.map(|u| u.version.clone()),
            notes: update.and_then(|u| u.body.clone()),
            date: update.and_then(|u| u.date.map(|d| d.to_string())),
            checked: state.checked,
            ready: update.is_some() && state.package.is_some(),
            installable: is_installable(),
        }
    }

    fn emit_status<R: Runtime>(&self, app: &AppHandle<R>) {
        let _ = app.emit(UPDATER_STATUS, self.status(app));
    }

    /// Asks the background task to run a check now, for example after the mode changed.
    pub fn wake(&self) {
        self.wake.notify_one();
    }

    /// Contacts the update server and records the result.
    pub async fn check<R: Runtime>(&self, app: &AppHandle<R>) -> AppResult<UpdateInfo> {
        if !is_configured(app) {
            return Err(not_configured());
        }
        let _busy = self.busy.lock().await;
        let updater = app
            .updater()
            .map_err(|e| not_configured().with_detail(e.to_string()))?;
        let found = updater.check().await.map_err(|e| {
            AppError::new(ErrorCode::Network, "Could not check for updates.")
                .with_detail(e.to_string())
        })?;
        {
            let mut state = self.lock();
            state.checked = true;
            let same = matches!(
                (&state.update, &found),
                (Some(old), Some(new)) if old.version == new.version
            );
            if !same {
                state.package = None;
            }
            state.update = found;
        }
        self.emit_status(app);
        Ok(self.status(app))
    }

    /// Downloads and verifies the available update, keeping it ready to install.
    pub async fn download<R: Runtime>(&self, app: &AppHandle<R>) -> AppResult<()> {
        if !is_installable() {
            return Err(not_installable());
        }
        let _busy = self.busy.lock().await;
        let update = {
            let mut state = self.lock();
            if state.package.is_some() || state.downloading {
                return Ok(());
            }
            let Some(update) = state.update.clone() else {
                return Ok(());
            };
            state.downloading = true;
            update
        };
        let mut downloaded = 0u64;
        let handle = app.clone();
        let result = update
            .download(
                |chunk, total| {
                    downloaded += chunk as u64;
                    let _ = handle.emit(
                        UPDATER_PROGRESS,
                        UpdateProgress {
                            downloaded,
                            total,
                            finished: false,
                        },
                    );
                },
                || {
                    let _ = handle.emit(
                        UPDATER_PROGRESS,
                        UpdateProgress {
                            downloaded: 0,
                            total: None,
                            finished: true,
                        },
                    );
                },
            )
            .await;
        {
            let mut state = self.lock();
            state.downloading = false;
            if let Ok(bytes) = &result {
                // Keep the package only if it still belongs to the current update.
                if state
                    .update
                    .as_ref()
                    .is_some_and(|u| u.version == update.version)
                {
                    state.package = Some(bytes.clone());
                }
            }
        }
        result.map_err(|e| {
            AppError::new(
                ErrorCode::UpdateUnavailable,
                "The update could not be downloaded or its signature is invalid.",
            )
            .with_detail(e.to_string())
        })?;
        self.emit_status(app);
        Ok(())
    }

    /// Makes sure the available update is downloaded and verified, ready for [`Self::install`].
    pub async fn prepare_install<R: Runtime>(&self, app: &AppHandle<R>) -> AppResult<()> {
        if !is_configured(app) {
            return Err(not_configured());
        }
        if !is_installable() {
            return Err(not_installable());
        }
        if self.lock().update.is_none() {
            self.check(app).await?;
        }
        if self.lock().update.is_none() {
            return Err(AppError::new(
                ErrorCode::UpdateUnavailable,
                "DarkLoad is already up to date.",
            ));
        }
        self.download(app).await
    }

    /// Installs a downloaded update while DarkLoad is closing, without reopening it.
    pub fn install_on_exit(&self) {
        if self.lock().package.is_none() || !is_installable() {
            return;
        }
        tracing::info!("installing the downloaded update before exit");
        if let Err(e) = self.install(false) {
            tracing::error!("could not install the update on exit: {}", e.message);
        }
    }

    /// Installs the downloaded update. On Windows the installer takes over and DarkLoad exits
    /// immediately (reopening it when `restart` is set); elsewhere this returns afterwards.
    pub fn install(&self, restart: bool) -> AppResult<()> {
        let (update, bytes) = {
            let state = self.lock();
            match (&state.update, &state.package) {
                (Some(u), Some(b)) => (u.clone(), b.clone()),
                _ => {
                    return Err(AppError::new(
                        ErrorCode::UpdateUnavailable,
                        "The update is not downloaded yet.",
                    ))
                }
            }
        };
        update
            .restart_after_install(restart)
            .install(&bytes)
            .map_err(|e| {
                AppError::new(
                    ErrorCode::UpdateUnavailable,
                    "The update could not be installed.",
                )
                .with_detail(e.to_string())
            })?;
        // The installer replaced the application files; forget the package so it is not
        // installed twice (Linux returns here, Windows has already exited).
        self.lock().package = None;
        Ok(())
    }

    /// Runs background checks for the lifetime of the application.
    pub fn start<R: Runtime>(self: &Arc<Self>, app: AppHandle<R>, settings: Arc<RwLock<Settings>>) {
        if !is_configured(&app) {
            return;
        }
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut delay = FIRST_CHECK_DELAY;
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = this.wake.notified() => {}
                }
                let mode = settings
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .update_mode;
                delay = CHECK_INTERVAL;
                if mode == UpdateMode::Off {
                    continue;
                }
                match this.check(&app).await {
                    Ok(info) if info.available && mode == UpdateMode::Auto && info.installable => {
                        if let Err(e) = this.download(&app).await {
                            tracing::warn!("background update download failed: {}", e.message);
                            delay = RETRY_DELAY;
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!("background update check failed: {}", e.message);
                        delay = RETRY_DELAY;
                    }
                }
            }
        });
    }
}
