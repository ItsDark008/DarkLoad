//! Signed application updates through the Tauri updater.
//!
//! The updater is only active in builds that were configured with an update endpoint and a
//! public signing key (see `docs/RELEASING.md`). Other builds report that updates are unavailable
//! instead of pretending to check.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_updater::UpdaterExt;

use crate::errors::{AppError, AppResult, ErrorCode};
use crate::events::UPDATER_PROGRESS;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub available: bool,
    pub current_version: String,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub date: Option<String>,
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

fn not_configured() -> AppError {
    AppError::new(
        ErrorCode::UpdateUnavailable,
        "Automatic updates are not configured for this build. Download new versions from the project's releases page.",
    )
}

pub async fn check<R: Runtime>(app: &AppHandle<R>) -> AppResult<UpdateInfo> {
    let current_version = app.package_info().version.to_string();
    if !is_configured(app) {
        return Err(not_configured());
    }
    let updater = app
        .updater()
        .map_err(|e| not_configured().with_detail(e.to_string()))?;
    let update = updater.check().await.map_err(|e| {
        AppError::new(ErrorCode::Network, "Could not check for updates.").with_detail(e.to_string())
    })?;
    Ok(match update {
        Some(u) => UpdateInfo {
            available: true,
            current_version,
            version: Some(u.version.clone()),
            notes: u.body.clone(),
            date: u.date.map(|d| d.to_string()),
        },
        None => UpdateInfo {
            available: false,
            current_version,
            version: None,
            notes: None,
            date: None,
        },
    })
}

pub async fn install<R: Runtime>(app: &AppHandle<R>) -> AppResult<()> {
    if !is_configured(app) {
        return Err(not_configured());
    }
    let updater = app
        .updater()
        .map_err(|e| not_configured().with_detail(e.to_string()))?;
    let update = updater
        .check()
        .await
        .map_err(|e| {
            AppError::new(ErrorCode::Network, "Could not check for updates.")
                .with_detail(e.to_string())
        })?
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::UpdateUnavailable,
                "DarkLoad is already up to date.",
            )
        })?;
    let mut downloaded = 0u64;
    let handle = app.clone();
    update
        .download_and_install(
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
        .await
        .map_err(|e| {
            AppError::new(
                ErrorCode::UpdateUnavailable,
                "The update could not be installed. Its signature may be invalid.",
            )
            .with_detail(e.to_string())
        })?;
    Ok(())
}
