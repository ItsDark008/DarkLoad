//! Optional platform accounts.
//!
//! Some platforms (TikTok and Instagram in particular) refuse anonymous access to public media.
//! Users can connect their own account by signing in on the platform's real login page, shown in
//! a DarkLoad window. DarkLoad never sees the password: the platform's session cookies live in the
//! application's private webview storage, exactly like a browser profile. When a download from that
//! platform starts, the session is exported to a temporary Netscape cookie file for yt-dlp and the
//! file is deleted when the operation ends.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use tauri::webview::Cookie;
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::errors::{AppError, AppResult, ErrorCode};
use crate::platform::Platform;

pub const ACCOUNTS_CHANGED: &str = "accounts://changed";
pub const COOKIE_FILE: &str = ".darkload-cookies.txt";

/// Sign-in details for a platform that supports account connection.
struct Site {
    cookie_url: &'static str,
    login_url: &'static str,
    session_cookie: &'static str,
}

fn site(platform: Platform) -> Option<Site> {
    match platform {
        Platform::Tiktok => Some(Site {
            cookie_url: "https://www.tiktok.com/",
            login_url: "https://www.tiktok.com/login",
            session_cookie: "sessionid",
        }),
        Platform::Instagram => Some(Site {
            cookie_url: "https://www.instagram.com/",
            login_url: "https://www.instagram.com/accounts/login/",
            session_cookie: "sessionid",
        }),
        Platform::Youtube => None,
    }
}

pub fn supports(platform: Platform) -> bool {
    site(platform).is_some()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    pub platform: Platform,
    pub connected: bool,
    pub signing_in: bool,
}

fn window_label(platform: Platform) -> String {
    format!("signin-{}", platform.as_str())
}

/// Reads the platform's cookies from the application's shared webview cookie store.
///
/// WebView2 deadlocks if cookies are read on the main thread, so the read happens on a blocking
/// worker thread.
async fn read_cookies<R: Runtime>(
    app: &AppHandle<R>,
    platform: Platform,
) -> AppResult<Vec<Cookie<'static>>> {
    let site = site(platform)
        .ok_or_else(|| AppError::invalid_input("This platform does not use account sign-in."))?;
    let window = app
        .get_webview_window(&window_label(platform))
        .or_else(|| app.get_webview_window("main"))
        .ok_or_else(|| AppError::internal("The application window is not available."))?;
    let url = tauri::Url::parse(site.cookie_url).expect("static URL");
    tauri::async_runtime::spawn_blocking(move || window.cookies_for_url(url))
        .await
        .map_err(|e| {
            AppError::internal("Could not read the account session.").with_detail(e.to_string())
        })?
        .map_err(|e| {
            AppError::internal("Could not read the account session.").with_detail(e.to_string())
        })
}

fn has_session(cookies: &[Cookie<'_>], platform: Platform) -> bool {
    let Some(site) = site(platform) else {
        return false;
    };
    cookies
        .iter()
        .any(|c| c.name() == site.session_cookie && !c.value().is_empty())
}

pub async fn status<R: Runtime>(app: &AppHandle<R>) -> Vec<AccountStatus> {
    let mut out = Vec::new();
    for platform in [Platform::Tiktok, Platform::Instagram] {
        let connected = read_cookies(app, platform)
            .await
            .map(|c| has_session(&c, platform))
            .unwrap_or(false);
        out.push(AccountStatus {
            platform,
            connected,
            signing_in: app.get_webview_window(&window_label(platform)).is_some(),
        });
    }
    out
}

/// Opens the platform's sign-in page in a DarkLoad window. The window has no access to
/// DarkLoad's commands; it closes itself once the platform reports a signed-in session.
pub async fn connect<R: Runtime>(app: &AppHandle<R>, platform: Platform) -> AppResult<()> {
    let site = site(platform)
        .ok_or_else(|| AppError::invalid_input("This platform does not use account sign-in."))?;
    let label = window_label(platform);
    if let Some(existing) = app.get_webview_window(&label) {
        let _ = existing.set_focus();
        return Ok(());
    }
    let url = tauri::Url::parse(site.login_url).expect("static URL");
    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(url))
        .title(format!("Sign in to {} — DarkLoad", platform.display_name()))
        .inner_size(480.0, 760.0)
        .min_inner_size(380.0, 560.0)
        .center()
        .build()
        .map_err(|e| {
            AppError::internal("Could not open the sign-in window.").with_detail(e.to_string())
        })?;

    let handle = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            let _ = handle.emit(ACCOUNTS_CHANGED, ());
        }
    });
    let _ = app.emit(ACCOUNTS_CHANGED, ());

    // Watch for a completed sign-in and close the window automatically.
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let Some(window) = handle.get_webview_window(&label) else {
                break;
            };
            if read_cookies(&handle, platform)
                .await
                .map(|c| has_session(&c, platform))
                .unwrap_or(false)
            {
                // Give the site a moment to finish writing its cookies before closing.
                tokio::time::sleep(Duration::from_secs(2)).await;
                let _ = window.close();
                let _ = handle.emit(ACCOUNTS_CHANGED, ());
                break;
            }
        }
    });
    Ok(())
}

/// Removes every cookie DarkLoad's webview holds for the platform.
pub async fn disconnect<R: Runtime>(app: &AppHandle<R>, platform: Platform) -> AppResult<()> {
    if let Some(window) = app.get_webview_window(&window_label(platform)) {
        let _ = window.close();
    }
    let cookies = read_cookies(app, platform).await?;
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| AppError::internal("The application window is not available."))?;
    tauri::async_runtime::spawn_blocking(move || {
        for cookie in cookies {
            let _ = window.delete_cookie(cookie);
        }
    })
    .await
    .map_err(|e| {
        AppError::internal("Could not remove the account session.").with_detail(e.to_string())
    })?;
    let _ = app.emit(ACCOUNTS_CHANGED, ());
    Ok(())
}

/// Exports the platform session in Netscape cookie-file format, or `None` when not connected.
pub async fn export<R: Runtime>(app: &AppHandle<R>, platform: Platform) -> Option<String> {
    if !supports(platform) {
        return None;
    }
    let cookies = read_cookies(app, platform).await.ok()?;
    has_session(&cookies, platform).then(|| to_netscape(&cookies, platform))
}

/// Formats cookies as a Netscape cookie file understood by yt-dlp.
pub fn to_netscape(cookies: &[Cookie<'_>], platform: Platform) -> String {
    let default_domain = site(platform)
        .and_then(|s| tauri::Url::parse(s.cookie_url).ok())
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default();
    let mut out = String::from("# Netscape HTTP Cookie File\n# Generated by DarkLoad for a single operation; deleted afterwards.\n\n");
    for c in cookies {
        let value = c.value();
        if c.name().is_empty() || c.name().contains(['\t', '\n']) || value.contains(['\t', '\n']) {
            continue;
        }
        // A Domain attribute (stored without its leading dot) means the cookie applies to
        // subdomains; cookies without one are host-only.
        let (domain, include_subdomains) = match c.domain() {
            Some(d) => (format!(".{}", d.trim_start_matches('.')), "TRUE"),
            None => (default_domain.clone(), "FALSE"),
        };
        let expires = c
            .expires_datetime()
            .map(|t| t.unix_timestamp().max(0))
            .unwrap_or(0);
        let prefix = if c.http_only().unwrap_or(false) {
            "#HttpOnly_"
        } else {
            ""
        };
        out.push_str(&format!(
            "{prefix}{domain}\t{include_subdomains}\t{}\t{}\t{expires}\t{}\t{value}\n",
            c.path().unwrap_or("/"),
            if c.secure().unwrap_or(false) {
                "TRUE"
            } else {
                "FALSE"
            },
            c.name(),
        ));
    }
    out
}

/// Writes a cookie file with owner-only permissions where the platform supports it.
pub fn write_cookie_file(dir: &Path, contents: &str) -> AppResult<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(COOKIE_FILE);
    std::fs::write(&path, contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(path)
}

/// Hint shown when a platform refuses anonymous access.
pub fn login_hint(platform: Platform) -> Option<AppError> {
    supports(platform).then(|| {
        AppError::new(
            ErrorCode::LoginRequired,
            format!(
                "{} requires a signed-in account for this content. Connect your account in Settings → Accounts, then try again.",
                platform.display_name()
            ),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_tiktok_and_instagram_support_accounts() {
        assert!(supports(Platform::Tiktok));
        assert!(supports(Platform::Instagram));
        assert!(!supports(Platform::Youtube));
    }

    #[test]
    fn formats_netscape_cookie_files() {
        let session = Cookie::build(("sessionid", "abc123"))
            .domain(".tiktok.com")
            .path("/")
            .secure(true)
            .http_only(true)
            .build();
        let host_only = Cookie::build(("tt_csrf", "x")).path("/").build();
        let malformed = Cookie::build(("bad", "a\tb")).build();
        let text = to_netscape(&[session, host_only, malformed], Platform::Tiktok);
        assert!(text.starts_with("# Netscape HTTP Cookie File"));
        assert!(text.contains("#HttpOnly_.tiktok.com\tTRUE\t/\tTRUE\t0\tsessionid\tabc123\n"));
        assert!(text.contains("www.tiktok.com\tFALSE\t/\tFALSE\t0\ttt_csrf\tx\n"));
        assert!(!text.contains("bad"));
    }

    #[test]
    fn detects_sessions() {
        let with = vec![Cookie::new("sessionid", "v")];
        let empty = vec![Cookie::new("sessionid", "")];
        let other = vec![Cookie::new("csrftoken", "v")];
        assert!(has_session(&with, Platform::Instagram));
        assert!(!has_session(&empty, Platform::Instagram));
        assert!(!has_session(&other, Platform::Tiktok));
        assert!(!has_session(&with, Platform::Youtube));
    }

    #[test]
    fn cookie_files_are_written_and_named_predictably() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_cookie_file(dir.path(), "# Netscape HTTP Cookie File\n").unwrap();
        assert_eq!(path.file_name().unwrap(), COOKIE_FILE);
        assert!(path.is_file());
    }
}
