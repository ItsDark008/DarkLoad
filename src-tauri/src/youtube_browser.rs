//! Built-in YouTube browser.
//!
//! YouTube runs in a child webview of the main window, placed over the Browser page. The page has
//! no IPC access: an injected script reports selections by starting a navigation to a sentinel URL,
//! which [`on_navigation`](tauri::webview::WebviewBuilder::on_navigation) reads and cancels. Every
//! value from the page is validated here, and download URLs and thumbnails are rebuilt from the
//! video ID rather than taken from the page.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::webview::{Color, NewWindowResponse, PageLoadEvent, PermissionResponse};
use tauri::{
    AppHandle, Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, Rect, Webview,
    WebviewBuilder, WebviewUrl,
};
use tauri_plugin_opener::OpenerExt;
use url::Url;

use crate::errors::{AppError, AppResult};
use crate::platform::{self, Platform};

pub const LABEL: &str = "youtube-browser";
pub const EVENT: &str = "browser://event";
const MAIN: &str = "main";
const HOME: &str = "https://www.youtube.com/";
const SCRIPT: &str = include_str!("youtube_browser.js");
const REPORT_PLACEHOLDER: &str = "DARKLOAD_BROWSER_REPORT_URL";
const TEXT_PLACEHOLDER: &str = "DARKLOAD_BROWSER_TEXT";
pub const MAX_SELECTION: usize = 500;
const MAX_REPORT_BYTES: usize = 256 * 1024;
const MAX_MESSAGES: usize = 64;
const MAX_TITLE_CHARS: usize = 300;
const MAX_UPLOADER_CHARS: usize = 120;
const MAX_INPUT_CHARS: usize = 2048;
/// Hosts that may load in the browser's top frame. Everything else is blocked and offered to the
/// system browser instead.
const ALLOWED_DOMAINS: &[&str] = &["youtube.com"];

/// Where the webview sits inside the main window, in logical pixels.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Bounds {
    fn rect(self) -> AppResult<Rect> {
        let valid = |v: f64| v.is_finite() && (0.0..=100_000.0).contains(&v);
        if !(valid(self.x) && valid(self.y) && valid(self.width) && valid(self.height)) {
            return Err(AppError::invalid_input("The browser area is out of range."));
        }
        Ok(Rect {
            position: LogicalPosition::new(self.x, self.y).into(),
            size: LogicalSize::new(self.width.max(1.0), self.height.max(1.0)).into(),
        })
    }
}

/// A video the person picked in the browser.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserVideo {
    pub id: String,
    pub url: String,
    pub title: Option<String>,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    pub thumbnail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BrowserEvent {
    /// A video was ticked or unticked in the page.
    Toggle {
        video: BrowserVideo,
        selected: bool,
    },
    /// The page changed, including YouTube's in-page navigations.
    Page {
        url: String,
        title: Option<String>,
        video: Option<BrowserVideo>,
    },
    Loading {
        loading: bool,
    },
    /// A link left YouTube; the frontend can offer to open it in the system browser.
    Blocked {
        url: String,
    },
}

#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "camelCase")]
enum RawMessage {
    Toggle {
        video: RawVideo,
        selected: bool,
    },
    Page {
        url: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        video: Option<RawVideo>,
    },
}

#[derive(Deserialize)]
struct RawVideo {
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    uploader: Option<String>,
    #[serde(default)]
    duration: Option<f64>,
}

#[derive(Default)]
struct Inner {
    webview: Option<Webview>,
    blocked: Option<Url>,
    selection: Vec<String>,
    /// Whether the Browser page currently wants the webview on screen.
    visible: bool,
}

pub struct YoutubeBrowser {
    app: AppHandle,
    report_url: String,
    inner: Mutex<Inner>,
    creating: tokio::sync::Mutex<()>,
}

impl YoutubeBrowser {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            report_url: format!(
                "https://darkload.invalid/browser-{}",
                uuid::Uuid::new_v4().simple()
            ),
            inner: Mutex::new(Inner::default()),
            creating: tokio::sync::Mutex::new(()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn webview(&self) -> Option<Webview> {
        self.lock().webview.clone()
    }

    fn require(&self) -> AppResult<Webview> {
        self.webview()
            .ok_or_else(|| AppError::invalid_input("The browser is not open."))
    }

    /// Shows the browser at `bounds`, creating it on first use.
    pub async fn show(&self, bounds: Bounds) -> AppResult<()> {
        let rect = bounds.rect()?;
        self.lock().visible = true;
        let _creating = self.creating.lock().await;
        if !self.lock().visible {
            return Ok(());
        }
        if let Some(webview) = self.webview() {
            webview.set_bounds(rect)?;
            webview.show()?;
            return Ok(());
        }
        let window = self
            .app
            .get_window(MAIN)
            .ok_or_else(|| AppError::internal("The main window is not available."))?;
        let webview = window.add_child(self.builder(), rect.position, rect.size)?;
        let mut inner = self.lock();
        // The page may have been left while the webview was being created.
        if !inner.visible {
            webview.hide()?;
        }
        inner.webview = Some(webview);
        Ok(())
    }

    pub fn hide(&self) -> AppResult<()> {
        let webview = {
            let mut inner = self.lock();
            inner.visible = false;
            inner.webview.clone()
        };
        if let Some(webview) = webview {
            // Hidden pages keep running; stop anything that is playing.
            let _ =
                webview.eval("document.querySelectorAll('video,audio').forEach((m)=>m.pause())");
            webview.hide()?;
        }
        Ok(())
    }

    pub fn set_bounds(&self, bounds: Bounds) -> AppResult<()> {
        let rect = bounds.rect()?;
        if let Some(webview) = self.webview() {
            webview.set_bounds(rect)?;
        }
        Ok(())
    }

    pub fn navigate(&self, input: &str) -> AppResult<()> {
        let url = resolve_input(input)?;
        self.require()?.navigate(url)?;
        Ok(())
    }

    pub fn back(&self) -> AppResult<()> {
        self.require()?.eval("history.back()")?;
        Ok(())
    }

    pub fn forward(&self) -> AppResult<()> {
        self.require()?.eval("history.forward()")?;
        Ok(())
    }

    pub fn reload(&self) -> AppResult<()> {
        self.require()?.reload()?;
        Ok(())
    }

    /// Tells the page which videos are selected, after the tray changed.
    pub fn sync_selection(&self, ids: Vec<String>) -> AppResult<()> {
        let ids = clean_selection(ids)?;
        let webview = {
            let mut inner = self.lock();
            inner.selection = ids.clone();
            inner.webview.clone()
        };
        if let Some(webview) = webview {
            webview.eval(sync_script(&ids))?;
        }
        Ok(())
    }

    /// Opens the last link that was blocked for leaving YouTube in the system browser.
    pub fn open_blocked(&self) -> AppResult<()> {
        let url = self
            .lock()
            .blocked
            .take()
            .ok_or_else(|| AppError::invalid_input("There is no link to open."))?;
        self.app
            .opener()
            .open_url(url.as_str(), None::<&str>)
            .map_err(|e| {
                AppError::internal("The link could not be opened.").with_detail(e.to_string())
            })
    }

    fn emit(app: &AppHandle, event: &BrowserEvent) {
        let _ = app.emit_to(EventTarget::webview(MAIN), EVENT, event);
    }

    fn block(app: &AppHandle, url: &Url) {
        if !matches!(url.scheme(), "http" | "https") {
            return;
        }
        if let Some(state) = app.try_state::<crate::commands::AppState>() {
            state.browser.lock().blocked = Some(url.clone());
        }
        Self::emit(
            app,
            &BrowserEvent::Blocked {
                url: url.to_string(),
            },
        );
    }

    fn builder(&self) -> WebviewBuilder<tauri::Wry> {
        let home = Url::parse(HOME).expect("static URL");
        let language = self
            .app
            .try_state::<crate::commands::AppState>()
            .map(|state| {
                state
                    .settings
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .language
            })
            .unwrap_or(crate::settings::Language::System);
        let script = SCRIPT
            .replace(REPORT_PLACEHOLDER, &self.report_url)
            .replace(TEXT_PLACEHOLDER, &crate::i18n::browser_text(language));
        let report_url = self.report_url.clone();
        let nav_app = self.app.clone();
        let window_app = self.app.clone();
        let load_app = self.app.clone();
        WebviewBuilder::new(LABEL, WebviewUrl::External(home))
            // Nothing from the browsing session is kept after DarkLoad closes.
            .incognito(true)
            .background_color(Color(0, 0, 0, 255))
            .disable_drag_drop_handler()
            .initialization_script(script)
            .on_permission_request(|_, _| PermissionResponse::Deny)
            .on_download(|_, _| false)
            .on_new_window(move |url, _| {
                if is_allowed(&url) {
                    if let Some(webview) = window_app.get_webview(LABEL) {
                        let _ = webview.navigate(url);
                    }
                } else {
                    Self::block(&window_app, &url);
                }
                NewWindowResponse::Deny
            })
            .on_navigation(move |url| {
                if let Some(raw) = report_payload(url, &report_url) {
                    for event in parse_report(&raw) {
                        Self::emit(&nav_app, &event);
                    }
                    return false;
                }
                if is_allowed(url) {
                    return true;
                }
                Self::block(&nav_app, url);
                false
            })
            .on_page_load(move |webview, payload| match payload.event() {
                PageLoadEvent::Started => {
                    Self::emit(&load_app, &BrowserEvent::Loading { loading: true })
                }
                PageLoadEvent::Finished => {
                    Self::emit(&load_app, &BrowserEvent::Loading { loading: false });
                    if let Some(state) = load_app.try_state::<crate::commands::AppState>() {
                        let ids = state.browser.lock().selection.clone();
                        let _ = webview.eval(sync_script(&ids));
                    }
                }
            })
    }
}

fn sync_script(ids: &[String]) -> String {
    // IDs are validated, so the JSON array cannot break out of the call.
    let ids = serde_json::to_string(ids).unwrap_or_else(|_| "[]".into());
    format!("window.__darkloadSync&&window.__darkloadSync({ids})")
}

pub fn is_allowed(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(|host| {
            ALLOWED_DOMAINS.iter().any(|domain| {
                host == *domain
                    || host
                        .strip_suffix(domain)
                        .is_some_and(|prefix| prefix.ends_with('.'))
            })
        })
}

/// Turns address-bar input into a YouTube URL: a YouTube link opens directly, anything else
/// becomes a search.
pub fn resolve_input(input: &str) -> AppResult<Url> {
    let input = input.trim();
    if input.chars().count() > MAX_INPUT_CHARS {
        return Err(AppError::invalid_input("That address is too long."));
    }
    if input.is_empty() {
        return Ok(Url::parse(HOME).expect("static URL"));
    }
    let looks_like_link = !input.contains(char::is_whitespace)
        && (input.contains("://") || (input.contains('.') && input.contains('/')) || {
            let host = input.split('/').next().unwrap_or("");
            host.ends_with("youtube.com") || host == "youtu.be"
        });
    if looks_like_link {
        let candidate = if input.contains("://") {
            input.to_string()
        } else {
            format!("https://{input}")
        };
        if let Ok(url) = Url::parse(&candidate) {
            if url.host_str() == Some("youtu.be") {
                if let Ok(detected) = platform::detect(url.as_str()) {
                    if detected.platform == Platform::Youtube {
                        return Url::parse(&detected.url)
                            .map_err(|_| AppError::invalid_input("That link is not valid."));
                    }
                }
            }
            let mut url = url;
            if url.scheme() == "http" {
                let _ = url.set_scheme("https");
            }
            if is_allowed(&url) {
                return Ok(url);
            }
        }
    }
    let mut search = Url::parse("https://www.youtube.com/results").expect("static URL");
    search.query_pairs_mut().append_pair("search_query", input);
    Ok(search)
}

fn report_payload(url: &Url, report_url: &str) -> Option<String> {
    let (page, fragment) = url.as_str().split_once('#')?;
    if page != report_url || fragment.len() > MAX_REPORT_BYTES * 3 {
        return None;
    }
    url::form_urlencoded::parse(fragment.as_bytes())
        .find(|(key, _)| key == "data")
        .map(|(_, value)| value.into_owned())
}

/// Validates a batch of messages from the page. Anything malformed is dropped.
fn parse_report(raw: &str) -> Vec<BrowserEvent> {
    if raw.len() > MAX_REPORT_BYTES {
        return Vec::new();
    }
    let Ok(messages) = serde_json::from_str::<Vec<serde_json::Value>>(raw) else {
        return Vec::new();
    };
    messages
        .into_iter()
        .take(MAX_MESSAGES)
        .filter_map(|value| serde_json::from_value::<RawMessage>(value).ok())
        .filter_map(|message| match message {
            RawMessage::Toggle { video, selected } => Some(BrowserEvent::Toggle {
                video: clean_video(video)?,
                selected,
            }),
            RawMessage::Page { url, title, video } => {
                let url = Url::parse(&url).ok().filter(is_allowed)?;
                Some(BrowserEvent::Page {
                    url: url.to_string(),
                    title: title.as_deref().and_then(|t| {
                        clean_text(t.trim_end_matches(" - YouTube"), MAX_TITLE_CHARS)
                    }),
                    video: video.and_then(clean_video),
                })
            }
        })
        .collect()
}

fn clean_video(raw: RawVideo) -> Option<BrowserVideo> {
    if !platform::is_youtube_video_id(&raw.id) {
        return None;
    }
    let id = raw.id;
    Some(BrowserVideo {
        url: format!("https://www.youtube.com/watch?v={id}"),
        thumbnail: format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg"),
        title: raw
            .title
            .as_deref()
            .and_then(|t| clean_text(t, MAX_TITLE_CHARS)),
        uploader: raw
            .uploader
            .as_deref()
            .and_then(|u| clean_text(u, MAX_UPLOADER_CHARS)),
        duration: raw
            .duration
            .filter(|d| d.is_finite() && *d > 0.0 && *d <= 7.0 * 24.0 * 3600.0),
        id,
    })
}

/// Collapses whitespace, strips control characters and caps the length.
fn clean_text(text: &str, max_chars: usize) -> Option<String> {
    let cleaned = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .take(max_chars)
        .collect::<String>();
    (!cleaned.is_empty()).then_some(cleaned)
}

fn clean_selection(ids: Vec<String>) -> AppResult<Vec<String>> {
    if ids.len() > MAX_SELECTION {
        return Err(AppError::invalid_input(format!(
            "Select at most {MAX_SELECTION} videos at a time."
        )));
    }
    let mut seen = std::collections::HashSet::new();
    let mut clean = Vec::with_capacity(ids.len());
    for id in ids {
        if !platform::is_youtube_video_id(&id) {
            return Err(AppError::invalid_input("A selected video ID is not valid."));
        }
        if seen.insert(id.clone()) {
            clean.push(id);
        }
    }
    Ok(clean)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = "https://darkload.invalid/browser-test";

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn only_youtube_pages_load_in_the_browser() {
        for ok in [
            "https://www.youtube.com/",
            "https://m.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtube.com/shorts/dQw4w9WgXcQ",
            "https://consent.youtube.com/m?continue=x",
        ] {
            assert!(is_allowed(&url(ok)), "{ok}");
        }
        for blocked in [
            "http://www.youtube.com/",
            "https://www.youtube.com:8443/",
            "https://user@www.youtube.com/",
            "https://notyoutube.com/",
            "https://youtube.com.example.net/",
            "https://accounts.google.com/ServiceLogin",
            "https://example.com/",
            "file:///C:/Windows",
            "javascript:alert(1)",
        ] {
            assert!(!is_allowed(&url(blocked)), "{blocked}");
        }
    }

    #[test]
    fn address_input_opens_links_or_searches() {
        assert_eq!(resolve_input("").unwrap().as_str(), HOME);
        assert_eq!(
            resolve_input("youtube.com/@channel").unwrap().as_str(),
            "https://youtube.com/@channel"
        );
        assert_eq!(
            resolve_input("http://www.youtube.com/watch?v=dQw4w9WgXcQ")
                .unwrap()
                .as_str(),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
        assert_eq!(
            resolve_input("https://youtu.be/dQw4w9WgXcQ?t=4")
                .unwrap()
                .as_str(),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
        assert_eq!(
            resolve_input("lofi hip hop").unwrap().as_str(),
            "https://www.youtube.com/results?search_query=lofi+hip+hop"
        );
        // Links that leave YouTube are searched for rather than opened.
        let search = resolve_input("https://example.com/page").unwrap();
        assert_eq!(search.host_str(), Some("www.youtube.com"));
        assert_eq!(search.path(), "/results");
        assert!(resolve_input(&"a".repeat(MAX_INPUT_CHARS + 1)).is_err());
    }

    #[test]
    fn reports_are_bound_to_this_browser() {
        let report = url(&format!("{REPORT}#data=%5B%5D"));
        assert_eq!(report_payload(&report, REPORT).as_deref(), Some("[]"));
        assert!(report_payload(&report, "https://darkload.invalid/browser-other").is_none());
        assert!(report_payload(&url(REPORT), REPORT).is_none());
    }

    #[test]
    fn page_messages_are_validated_and_rebuilt() {
        let raw = serde_json::json!([
            {"t": "toggle", "selected": true, "video": {
                "id": "dQw4w9WgXcQ", "title": "  A\u{0007} video\n title ", "uploader": "Chan",
                "duration": 212.0
            }},
            {"t": "toggle", "selected": true, "video": {"id": "bad id\"); alert(1); //"}},
            {"t": "toggle", "selected": false, "video": {"id": "abcdefghijk", "duration": -4}},
            {"t": "page", "url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
             "title": "Song - YouTube", "video": {"id": "dQw4w9WgXcQ"}},
            {"t": "page", "url": "https://evil.example/"},
            {"t": "unknown"}
        ])
        .to_string();
        let events = parse_report(&raw);
        assert_eq!(events.len(), 3);
        let BrowserEvent::Toggle { video, selected } = &events[0] else {
            panic!("expected a toggle");
        };
        assert!(selected);
        assert_eq!(video.url, "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(
            video.thumbnail,
            "https://i.ytimg.com/vi/dQw4w9WgXcQ/mqdefault.jpg"
        );
        assert_eq!(video.title.as_deref(), Some("A video title"));
        assert_eq!(video.duration, Some(212.0));
        let BrowserEvent::Toggle { video, selected } = &events[1] else {
            panic!("expected a toggle");
        };
        assert!(!selected);
        assert_eq!(video.duration, None);
        let BrowserEvent::Page { title, video, .. } = &events[2] else {
            panic!("expected a page");
        };
        assert_eq!(title.as_deref(), Some("Song"));
        assert_eq!(video.as_ref().map(|v| v.id.as_str()), Some("dQw4w9WgXcQ"));
        assert!(parse_report("not json").is_empty());
        assert!(parse_report(&"x".repeat(MAX_REPORT_BYTES + 1)).is_empty());
    }

    #[test]
    fn selection_sync_only_accepts_video_ids() {
        assert_eq!(
            clean_selection(vec!["dQw4w9WgXcQ".into(), "dQw4w9WgXcQ".into()]).unwrap(),
            vec!["dQw4w9WgXcQ".to_string()]
        );
        assert!(clean_selection(vec!["\"]);alert(1)//".into()]).is_err());
        assert!(clean_selection(vec!["dQw4w9WgXcQ".into(); MAX_SELECTION + 1]).is_err());
        assert_eq!(
            sync_script(&["dQw4w9WgXcQ".into()]),
            r#"window.__darkloadSync&&window.__darkloadSync(["dQw4w9WgXcQ"])"#
        );
    }

    #[test]
    fn bounds_must_be_finite_and_in_range() {
        let ok = Bounds {
            x: 228.0,
            y: 84.0,
            width: 700.0,
            height: 500.0,
        };
        assert!(ok.rect().is_ok());
        for bad in [f64::NAN, f64::INFINITY, -1.0, 1e9] {
            assert!(Bounds { x: bad, ..ok }.rect().is_err());
            assert!(Bounds { width: bad, ..ok }.rect().is_err());
        }
    }
}
