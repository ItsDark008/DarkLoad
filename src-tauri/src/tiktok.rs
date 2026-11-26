//! Optional fallback for public videos served by TikTok's guest player.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::webview::{NewWindowResponse, PageLoadEvent, PermissionResponse};
use tauri::{AppHandle, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tempfile::NamedTempFile;
use tokio::sync::{oneshot, Semaphore};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::errors::{AppError, AppResult, ErrorCode};
use crate::extractor::{self, Analysis};
use crate::platform::{self, DetectedUrl, Platform, UrlKind};

const TIKTOK: &str = "https://www.tiktok.com";
const GUEST_WINDOW_PREFIX: &str = "guest-tiktok-";
const OBSERVER: &str = include_str!("tiktok_guest.js");
const COOKIE_NAMES: &[&str] = &["tt_chain_token", "ttwid", "msToken", "odin_tt"];
const CDN_DOMAINS: &[&str] = &[
    "tiktok.com",
    "tiktokcdn.com",
    "tiktokcdn-eu.com",
    "tiktokcdn-us.com",
    "tiktokv.com",
    "tiktokv.eu",
];

pub fn is_guest_window(label: &str) -> bool {
    label.starts_with(GUEST_WINDOW_PREFIX)
}

pub struct GuestPlayer {
    app: AppHandle,
    slots: Semaphore,
    shortlinks: reqwest::Client,
}

pub struct GuestVideo {
    detected: DetectedUrl,
    info: Value,
    cookies: String,
}

impl GuestVideo {
    pub fn analysis(&self) -> AppResult<Analysis> {
        extractor::parse_analysis(&self.info, &self.detected)
    }

    pub fn download_files(&self) -> AppResult<GuestDownloadFiles> {
        let mut info = NamedTempFile::new()?;
        serde_json::to_writer(info.as_file_mut(), &self.info)?;
        let mut cookies = NamedTempFile::new()?;
        cookies.write_all(self.cookies.as_bytes())?;
        Ok(GuestDownloadFiles { info, cookies })
    }
}

pub struct GuestDownloadFiles {
    info: NamedTempFile,
    cookies: NamedTempFile,
}

impl GuestDownloadFiles {
    pub fn info_path(&self) -> &Path {
        self.info.path()
    }

    pub fn cookies_path(&self) -> &Path {
        self.cookies.path()
    }
}

struct GuestWindow(WebviewWindow);

impl Drop for GuestWindow {
    fn drop(&mut self) {
        let _ = self.0.destroy();
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayerResponse {
    data: Value,
    user_agent: String,
}

impl GuestPlayer {
    pub fn new(app: AppHandle) -> AppResult<Self> {
        let shortlinks = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(8))
            .build()?;
        Ok(Self {
            app,
            slots: Semaphore::new(1),
            shortlinks,
        })
    }

    pub fn can_try(detected: &DetectedUrl, error: &AppError) -> bool {
        detected.platform == Platform::Tiktok
            && matches!(detected.kind, UrlKind::Video | UrlKind::Shortlink)
            && error.code == ErrorCode::LoginRequired
    }

    pub async fn resolve(
        &self,
        detected: &DetectedUrl,
        token: &CancellationToken,
    ) -> AppResult<GuestVideo> {
        tokio::select! {
            biased;
            _ = token.cancelled() => Err(AppError::cancelled()),
            result = async {
                let _permit = self.slots.acquire().await.map_err(|_| AppError::cancelled())?;
                tokio::time::timeout(Duration::from_secs(30), self.resolve_inner(detected))
                    .await
                    .map_err(|_| AppError::new(ErrorCode::LoginRequired, "TikTok's guest player did not respond."))?
            } => result,
        }
    }

    async fn resolve_inner(&self, detected: &DetectedUrl) -> AppResult<GuestVideo> {
        let detected = self.canonical_video(detected).await?;
        let id = detected.id.as_deref().ok_or_else(unavailable)?;
        let label = format!("{GUEST_WINDOW_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let report_url = format!("https://darkload.invalid/{label}");
        let player_url = Url::parse(&format!("{TIKTOK}/player/v1/{id}?autoplay=1&muted=1"))
            .map_err(|_| unavailable())?;
        // The callback cancels this navigation before it can leave the WebView.
        let observer = OBSERVER.replace("DARKLOAD_GUEST_REPORT_URL", &report_url);
        let (send, receive) = oneshot::channel();
        let send = Mutex::new(Some(send));
        let warmup = Mutex::new(Some(player_url.clone()));
        let player_path = player_url.path().to_string();
        let window = GuestWindow(
            WebviewWindowBuilder::new(
                &self.app,
                &label,
                WebviewUrl::External(Url::parse(TIKTOK).expect("static URL")),
            )
            .visible(false)
            .focused(false)
            .incognito(true)
            .disable_drag_drop_handler()
            .on_new_window(|_, _| NewWindowResponse::Deny)
            .on_permission_request(|_, _| PermissionResponse::Deny)
            .on_download(|_, _| false)
            .initialization_script(observer)
            .on_navigation(move |url| {
                if let Some(payload) = report_payload(url, &report_url) {
                    if let Some(send) = send.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = send.send(payload);
                    }
                    return false;
                }
                is_tiktok_page(url)
                    && (matches!(url.path(), "/" | "/login" | "/login/")
                        || url.path() == player_path)
            })
            .on_page_load(move |window, payload| {
                if payload.event() == PageLoadEvent::Finished
                    && is_tiktok_page(payload.url())
                    && matches!(payload.url().path(), "/" | "/login" | "/login/")
                {
                    if let Some(next) = warmup.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = window.navigate(next);
                    }
                }
            })
            .build()?,
        );
        let raw = receive.await.map_err(|_| unavailable())?;
        let info = player_info(&raw, &detected)?;
        let urls: HashSet<_> = info["formats"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|format| format["url"].as_str())
            .filter_map(cdn_url)
            .map(|mut url| {
                url.set_query(None);
                url
            })
            .collect();
        let webview = window.0.clone();
        let cookies =
            tauri::async_runtime::spawn_blocking(move || read_guest_cookies(&webview, urls))
                .await
                .map_err(|_| unavailable())??;
        Ok(GuestVideo {
            detected,
            info,
            cookies,
        })
    }

    async fn canonical_video(&self, detected: &DetectedUrl) -> AppResult<DetectedUrl> {
        let mut current = detected.clone();
        for _ in 0..4 {
            if current.platform != Platform::Tiktok {
                break;
            }
            if current.kind == UrlKind::Video {
                return Ok(current);
            }
            if current.kind != UrlKind::Shortlink {
                break;
            }
            let url = Url::parse(&current.url).map_err(|_| unavailable())?;
            let response = self.shortlinks.get(url.clone()).send().await?;
            let Some(location) = response.headers().get(reqwest::header::LOCATION) else {
                break;
            };
            let next = url
                .join(location.to_str().map_err(|_| unavailable())?)
                .map_err(|_| unavailable())?;
            if next.scheme() != "https" {
                break;
            }
            current = platform::detect(next.as_str())?;
        }
        Err(unavailable())
    }
}

fn unavailable() -> AppError {
    AppError::new(
        ErrorCode::LoginRequired,
        "TikTok did not make this video available to its guest player.",
    )
}

fn is_tiktok_page(url: &Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("www.tiktok.com")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
}

fn report_payload(url: &Url, report_url: &str) -> Option<String> {
    let (page, fragment) = url.as_str().split_once('#')?;
    if page != report_url || fragment.len() > 800_000 {
        return None;
    }
    url::form_urlencoded::parse(fragment.as_bytes())
        .find(|(key, _)| key == "data")
        .map(|(_, value)| value.into_owned())
}

fn is_cdn_host(host: &str) -> bool {
    CDN_DOMAINS.iter().any(|domain| {
        host == *domain
            || host
                .strip_suffix(domain)
                .is_some_and(|prefix| prefix.ends_with('.'))
    })
}

fn cdn_url(raw: &str) -> Option<Url> {
    let url = Url::parse(raw).ok()?;
    let host = url.host_str()?;
    (url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && host != "www.tiktok.com"
        && is_cdn_host(host))
    .then_some(url)
}

fn post_matches(value: &Value, id: &str) -> bool {
    value["id_str"].as_str() == Some(id)
        || value["id"].as_str() == Some(id)
        || value["id"]
            .as_u64()
            .is_some_and(|value| value.to_string() == id)
}

fn player_info(raw: &str, detected: &DetectedUrl) -> AppResult<Value> {
    let response: PlayerResponse = serde_json::from_str(raw).map_err(|_| unavailable())?;
    let agent = response.user_agent.trim();
    if agent.is_empty()
        || agent.len() > 512
        || reqwest::header::HeaderValue::from_str(agent).is_err()
    {
        return Err(unavailable());
    }
    let id = detected.id.as_deref().ok_or_else(unavailable)?;
    let data = response.data;
    let status = data["results"].as_array().and_then(|results| {
        results
            .iter()
            .find(|result| post_matches(result, id))
            .map(|entry| &entry["code"])
    });
    if data["status_code"].as_i64() != Some(0)
        || !status.is_some_and(|code| code.as_str() == Some("ok") || code.as_i64() == Some(0))
    {
        return Err(unavailable());
    }
    let item = data["items"]
        .as_array()
        .and_then(|items| items.iter().find(|item| post_matches(item, id)))
        .ok_or_else(unavailable)?;
    let video = &item["video_info"];
    let meta = &video["meta"];
    let mut formats = Vec::new();
    let mut ids = HashSet::new();
    for profile in video["profiles"].as_array().into_iter().flatten().take(24) {
        let address = &profile["play_addr"];
        let Some(url) = media_url(address) else {
            continue;
        };
        let Some(gear) = profile["gear_name"].as_str().filter(|gear| {
            !gear.is_empty()
                && gear.len() <= 64
                && gear
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        }) else {
            continue;
        };
        let format_id = format!("guest-{gear}");
        if ids.insert(format_id.clone()) {
            formats.push(json!({
                "format_id": format_id, "url": url.as_str(), "ext": "mp4", "protocol": "https",
                "width": address["width"].as_u64(), "height": address["height"].as_u64(),
                "fps": profile["fps"].as_f64(), "filesize": address["data_size"].as_u64(),
                "tbr": profile["bitrate"].as_f64().map(|b| b / 1_000.0),
                "vcodec": match profile["codec_type"].as_str() {
                    Some("h264" | "avc1") => "h264",
                    Some("h265" | "hevc" | "bytevc1") => "hevc",
                    Some("av1") => "av1",
                    _ => "unknown",
                },
                "acodec": "unknown",
            }));
        }
    }
    if formats.is_empty() {
        let url = media_url(video).ok_or_else(unavailable)?;
        formats.push(json!({
            "format_id": "guest-mp4", "url": url.as_str(), "ext": "mp4", "protocol": "https",
            "width": meta["width"].as_u64(), "height": meta["height"].as_u64(),
            "vcodec": "unknown", "acodec": "unknown",
        }));
    }
    let thumbnail = video["cover"]["url_list"].as_array().and_then(|urls| {
        urls.iter()
            .filter_map(Value::as_str)
            .find(|url| cdn_url(url).is_some())
    });
    let author = &item["author_info"];
    let title = item["desc"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    Ok(json!({
        "id": id, "title": title.unwrap_or(id), "webpage_url": detected.url,
        "uploader": author["nickname"].as_str(), "uploader_id": author["unique_id"].as_str(),
        "duration": meta["duration"].as_f64(), "thumbnail": thumbnail,
        "formats": formats, "http_headers": {"Referer": format!("{TIKTOK}/"), "User-Agent": agent},
    }))
}

fn media_url(address: &Value) -> Option<Url> {
    address["url_list"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .filter_map(cdn_url)
        .find(|url| url.path().contains("/video/tos/"))
}

fn read_guest_cookies(webview: &WebviewWindow, urls: HashSet<Url>) -> AppResult<String> {
    let mut cookies = HashMap::new();
    for url in urls {
        let Some(host) = url.host_str() else {
            continue;
        };
        for cookie in webview.cookies_for_url(url.clone())? {
            if !COOKIE_NAMES.contains(&cookie.name()) || cookie.value().contains(['\t', '\r', '\n'])
            {
                continue;
            }
            let path = cookie.path().unwrap_or("/");
            if path.contains(['\t', '\r', '\n']) {
                continue;
            }
            let expires = cookie
                .expires_datetime()
                .map(|time| time.unix_timestamp())
                .unwrap_or(0);
            let secure = if cookie.secure().unwrap_or(false) {
                "TRUE"
            } else {
                "FALSE"
            };
            cookies.insert(
                (
                    host.to_string(),
                    path.to_string(),
                    cookie.name().to_string(),
                ),
                format!(
                    "{host}\tFALSE\t{path}\t{secure}\t{expires}\t{}\t{}\n",
                    cookie.name(),
                    cookie.value()
                ),
            );
        }
    }
    let mut output = String::from("# Netscape HTTP Cookie File\n");
    for cookie in cookies.into_values() {
        output.push_str(&cookie);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detected() -> DetectedUrl {
        platform::detect("https://www.tiktok.com/@example/video/1234567890123456789").unwrap()
    }

    fn player_response(id: &str) -> Value {
        json!({
            "userAgent": "Test client",
            "data": {
                "status_code": 0,
                "results": [{"id_str": id, "code": "ok"}],
                "items": [{
                    "id_str": id,
                    "desc": "Public video",
                    "video_info": {
                        "profiles": [{
                            "gear_name": "avc_540p",
                            "codec_type": "h264",
                            "fps": 30,
                            "play_addr": {
                                "url_list": [
                                    "https://tiktok.com.example.net/video/tos/fake.mp4",
                                    "https://v16-webapp-prime.tiktok.com/video/tos/valid.mp4"
                                ],
                                "width": 540,
                                "height": 960
                            }
                        }]
                    }
                }]
            }
        })
    }

    #[test]
    fn accepts_formats_only_from_the_requested_public_post() {
        let detected = detected();
        let mut response = player_response(detected.id.as_deref().unwrap());
        let info = player_info(&response.to_string(), &detected).unwrap();
        assert_eq!(info["formats"][0]["format_id"], "guest-avc_540p");
        assert_eq!(
            info["formats"][0]["url"],
            "https://v16-webapp-prime.tiktok.com/video/tos/valid.mp4"
        );
        let Analysis::Media(media) = extractor::parse_analysis(&info, &detected).unwrap() else {
            panic!("expected a single video");
        };
        let plan = crate::formats::plan(&media.formats, None, &Default::default()).unwrap();
        assert_eq!(plan.output_container, Some(crate::formats::Container::Mp4));
        assert!(!plan.requires_merge);

        response["data"]["results"][0]["code"] = json!("not_available");
        assert!(player_info(&response.to_string(), &detected).is_err());
        let different_post = player_response("1234567890123456790");
        assert!(player_info(&different_post.to_string(), &detected).is_err());
    }

    #[test]
    fn only_a_login_requirement_qualifies_for_the_guest_player() {
        let detected = detected();
        assert!(GuestPlayer::can_try(&detected, &unavailable()));
        for code in [
            ErrorCode::PrivateContent,
            ErrorCode::AgeRestricted,
            ErrorCode::GeoRestricted,
            ErrorCode::DrmProtected,
        ] {
            assert!(!GuestPlayer::can_try(&detected, &AppError::new(code, "")));
        }
    }

    #[test]
    fn guest_navigation_reports_are_bound_to_the_selected_window() {
        let report = "https://darkload.invalid/guest-tiktok-example";
        let url = Url::parse(&format!("{report}#data=%7B%22key%22%3A%22a%2Bb%22%7D")).unwrap();
        assert_eq!(
            report_payload(&url, report).as_deref(),
            Some(r#"{"key":"a+b"}"#)
        );
        assert!(report_payload(&url, "https://darkload.invalid/wrong").is_none());
    }
}
