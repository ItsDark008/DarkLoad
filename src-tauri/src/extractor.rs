//! Media analysis through yt-dlp's structured JSON output.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::errors::{AppError, AppResult, ErrorCode};
use crate::formats::{summarize, FormatSummary, MediaFormat};
use crate::platform::{DetectedUrl, Platform, UrlKind};
use crate::process;

/// Resolved executable locations used to build extractor command lines.
#[derive(Debug, Clone)]
pub struct ToolPaths {
    pub yt_dlp: PathBuf,
    pub ffmpeg: Option<PathBuf>,
    pub deno: Option<PathBuf>,
    pub cache_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleTrack {
    pub language: String,
    pub name: Option<String>,
    pub automatic: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub id: String,
    pub url: String,
    pub title: String,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
    pub thumbnail_local: Option<String>,
    pub platform: Platform,
    pub was_live: bool,
    pub upload_date: Option<String>,
    pub view_count: Option<u64>,
    pub chapters: u32,
    pub formats: Vec<MediaFormat>,
    pub summary: FormatSummary,
    pub subtitles: Vec<SubtitleTrack>,
    /// Set when this media belongs to a YouTube playlist the user may want instead.
    pub playlist_id: Option<String>,
    /// Index inside a multi-item post (Instagram carousels).
    pub playlist_item: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistEntry {
    pub id: String,
    pub url: String,
    pub title: String,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
    pub index: u32,
    pub playlist_item: Option<u32>,
    pub available: bool,
    /// Full media details, present when the extractor resolved them (carousels).
    pub media: Option<Box<MediaInfo>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistInfo {
    pub id: String,
    pub url: String,
    pub title: String,
    pub uploader: Option<String>,
    pub platform: Platform,
    pub thumbnail: Option<String>,
    pub entries: Vec<PlaylistEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Analysis {
    Media(Box<MediaInfo>),
    Playlist(PlaylistInfo),
}

/// Network options applied to every yt-dlp invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetOptions {
    pub socket_timeout: u32,
    pub force_ipv4: bool,
}

impl From<&crate::settings::Settings> for NetOptions {
    fn from(s: &crate::settings::Settings) -> Self {
        Self {
            socket_timeout: s.socket_timeout_secs,
            force_ipv4: s.prefer_ipv4,
        }
    }
}

/// Arguments shared by every yt-dlp invocation.
pub fn base_args(tools: &ToolPaths, net: NetOptions) -> Vec<OsString> {
    let socket_timeout = net.socket_timeout;
    let mut args: Vec<OsString> = vec![
        "--ignore-config".into(),
        "--color".into(),
        "never".into(),
        "--encoding".into(),
        "utf-8".into(),
        "--socket-timeout".into(),
        socket_timeout.to_string().into(),
        "--cache-dir".into(),
        tools.cache_dir.clone().into_os_string(),
    ];
    if net.force_ipv4 {
        args.push("--force-ipv4".into());
    }
    if let Some(ffmpeg) = &tools.ffmpeg {
        args.push("--ffmpeg-location".into());
        args.push(ffmpeg.clone().into_os_string());
    }
    if let Some(deno) = &tools.deno {
        let mut runtime = OsString::from("deno:");
        runtime.push(deno.as_os_str());
        args.push("--js-runtimes".into());
        args.push(runtime);
    }
    args
}

/// Runs yt-dlp in JSON mode and parses the result.
pub async fn analyze(
    tools: &ToolPaths,
    detected: &DetectedUrl,
    net: NetOptions,
    cancel: &CancellationToken,
) -> AppResult<Analysis> {
    let socket_timeout = net.socket_timeout;
    let mut args = base_args(tools, net);
    args.extend([
        "--dump-single-json".into(),
        "--no-progress".into(),
        "--no-warnings".into(),
    ]);
    if detected.kind == UrlKind::Playlist {
        args.push("--flat-playlist".into());
    } else {
        args.push("--no-playlist".into());
    }
    args.push("--".into());
    args.push(detected.url.clone().into());

    let timeout = Duration::from_secs(u64::from(socket_timeout).max(30) * 6);
    let finished = process::run(
        &tools.yt_dlp,
        &args,
        None,
        cancel,
        Some(timeout),
        true,
        |_| {},
    )
    .await?;
    if !finished.success {
        return Err(classify_error(&finished.stderr_tail));
    }
    let json: Value = serde_json::from_str(finished.stdout.trim()).map_err(|e| {
        AppError::new(
            ErrorCode::ProcessFailed,
            "The extractor returned data DarkLoad could not read.",
        )
        .with_detail(e.to_string())
    })?;
    parse_analysis(&json, detected)
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn f64_field(v: &Value, key: &str) -> Option<f64> {
    v.get(key)
        .and_then(Value::as_f64)
        .filter(|x| x.is_finite() && *x >= 0.0)
}

fn u32_field(v: &Value, key: &str) -> Option<u32> {
    v.get(key)
        .and_then(Value::as_u64)
        .and_then(|x| u32::try_from(x).ok())
        .filter(|x| *x > 0)
}

fn u64_field(v: &Value, key: &str) -> Option<u64> {
    v.get(key)
        .and_then(Value::as_u64)
        .filter(|x| *x > 0)
        .or_else(|| f64_field(v, key).map(|f| f as u64).filter(|x| *x > 0))
}

fn codec_field(v: &Value, key: &str) -> Option<String> {
    str_field(v, key).filter(|c| c != "none")
}

pub fn parse_format(v: &Value) -> Option<MediaFormat> {
    let id = str_field(v, "format_id")?;
    Some(MediaFormat {
        id,
        ext: str_field(v, "ext").unwrap_or_else(|| "unknown".into()),
        vcodec: codec_field(v, "vcodec").or_else(|| {
            // Some extractors omit codecs for progressive streams; infer video from dimensions.
            (str_field(v, "vcodec").is_none() && u32_field(v, "height").is_some())
                .then(|| "unknown".into())
        }),
        acodec: codec_field(v, "acodec").or_else(|| {
            (str_field(v, "acodec").is_none()
                && str_field(v, "vcodec").is_none()
                && u32_field(v, "height").is_some())
            .then(|| "unknown".into())
        }),
        width: u32_field(v, "width"),
        height: u32_field(v, "height"),
        fps: f64_field(v, "fps"),
        tbr: f64_field(v, "tbr"),
        vbr: f64_field(v, "vbr"),
        abr: f64_field(v, "abr"),
        asr: u32_field(v, "asr"),
        audio_channels: u32_field(v, "audio_channels"),
        filesize: u64_field(v, "filesize"),
        filesize_approx: u64_field(v, "filesize_approx"),
        dynamic_range: str_field(v, "dynamic_range"),
        protocol: str_field(v, "protocol"),
        note: str_field(v, "format_note"),
        language: str_field(v, "language"),
        language_preference: v
            .get("language_preference")
            .and_then(Value::as_i64)
            .map(|x| x.clamp(-100, 100) as i32),
    })
}

fn pick_thumbnail(v: &Value) -> Option<String> {
    let https = |s: String| s.starts_with("https://").then_some(s);
    if let Some(list) = v.get("thumbnails").and_then(Value::as_array) {
        // Prefer a mid-sized image for lists; fall back to the last (usually largest) entry.
        let sized = list
            .iter()
            .filter(|t| {
                t.get("url")
                    .and_then(Value::as_str)
                    .is_some_and(|u| u.starts_with("https://"))
            })
            .filter_map(|t| Some((u32_field(t, "width")?, str_field(t, "url")?)))
            .filter(|(w, _)| (320..=1280).contains(w))
            .min_by_key(|(w, _)| w.abs_diff(640));
        if let Some((_, url)) = sized {
            return Some(url);
        }
    }
    str_field(v, "thumbnail").and_then(https).or_else(|| {
        v.get("thumbnails").and_then(Value::as_array).and_then(|l| {
            l.iter()
                .rev()
                .find_map(|t| str_field(t, "url").and_then(https))
        })
    })
}

fn parse_subtitles(v: &Value) -> Vec<SubtitleTrack> {
    let mut tracks = Vec::new();
    for (key, automatic) in [("subtitles", false), ("automatic_captions", true)] {
        if let Some(map) = v.get(key).and_then(Value::as_object) {
            for (lang, entries) in map {
                if lang == "live_chat" || !crate::settings::is_language_code(lang) {
                    continue;
                }
                // Automatic captions include hundreds of machine translations; keep originals only.
                if automatic && lang.contains('-') && !lang.ends_with("-orig") {
                    continue;
                }
                let name = entries
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|e| str_field(e, "name"));
                tracks.push(SubtitleTrack {
                    language: lang.clone(),
                    name,
                    automatic,
                });
            }
        }
    }
    tracks.sort_by(|a, b| {
        a.automatic
            .cmp(&b.automatic)
            .then(a.language.cmp(&b.language))
    });
    tracks
}

fn parse_media(v: &Value, detected: &DetectedUrl, fallback_url: &str) -> AppResult<MediaInfo> {
    let live_status = str_field(v, "live_status");
    if v.get("is_live").and_then(Value::as_bool) == Some(true)
        || live_status.as_deref() == Some("is_live")
    {
        return Err(AppError::new(
            ErrorCode::LiveNotFinished,
            "This livestream is still in progress. Download it after it has ended.",
        ));
    }
    if live_status.as_deref() == Some("is_upcoming") {
        return Err(AppError::new(
            ErrorCode::LiveNotFinished,
            "This livestream has not started yet.",
        ));
    }
    let mut formats: Vec<MediaFormat> = v
        .get("formats")
        .and_then(Value::as_array)
        .map(|l| l.iter().filter_map(parse_format).collect())
        .unwrap_or_default();
    if formats.is_empty() {
        // Single-format extractors report the format at the top level.
        if let Some(f) = parse_format(v) {
            formats.push(f);
        }
    }
    let summary = summarize(&formats);
    if !summary.has_video && !summary.has_audio {
        return Err(AppError::new(
            ErrorCode::FormatUnavailable,
            "No downloadable video or audio was found for this link.",
        ));
    }
    let id = str_field(v, "id").unwrap_or_else(|| detected.id.clone().unwrap_or_default());
    Ok(MediaInfo {
        url: str_field(v, "webpage_url")
            .filter(|u| u.starts_with("https://"))
            .unwrap_or_else(|| fallback_url.to_string()),
        title: str_field(v, "title")
            .or_else(|| str_field(v, "fulltitle"))
            .unwrap_or_else(|| id.clone()),
        id,
        uploader: str_field(v, "uploader")
            .or_else(|| str_field(v, "channel"))
            .or_else(|| str_field(v, "creator")),
        duration: f64_field(v, "duration"),
        thumbnail: pick_thumbnail(v),
        thumbnail_local: None,
        platform: detected.platform,
        was_live: live_status.as_deref() == Some("was_live")
            || live_status.as_deref() == Some("post_live"),
        upload_date: str_field(v, "upload_date"),
        view_count: u64_field(v, "view_count"),
        chapters: v
            .get("chapters")
            .and_then(Value::as_array)
            .map(|c| c.len() as u32)
            .unwrap_or(0),
        formats,
        summary,
        subtitles: parse_subtitles(v),
        playlist_id: detected.playlist_id.clone(),
        playlist_item: None,
    })
}

fn is_unavailable_title(title: &str) -> bool {
    matches!(
        title,
        "[Deleted video]" | "[Private video]" | "[Unavailable video]"
    )
}

/// Converts yt-dlp JSON into an [`Analysis`].
pub fn parse_analysis(v: &Value, detected: &DetectedUrl) -> AppResult<Analysis> {
    let kind = str_field(v, "_type").unwrap_or_else(|| "video".into());
    if kind != "playlist" && kind != "multi_video" {
        return Ok(Analysis::Media(Box::new(parse_media(
            v,
            detected,
            &detected.url,
        )?)));
    }
    let entries_json = v
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let parent_url = str_field(v, "webpage_url").unwrap_or_else(|| detected.url.clone());
    let mut entries = Vec::new();
    for (i, e) in entries_json.iter().enumerate() {
        if e.is_null() {
            continue;
        }
        let index = (i + 1) as u32;
        let title = str_field(e, "title").unwrap_or_else(|| format!("Item {index}"));
        let resolved = e.get("formats").is_some();
        if resolved {
            // Fully resolved entry (for example an Instagram carousel item).
            match parse_media(e, detected, &parent_url) {
                Ok(mut media) => {
                    media.playlist_item = Some(index);
                    media.url = parent_url.clone();
                    entries.push(PlaylistEntry {
                        id: media.id.clone(),
                        url: parent_url.clone(),
                        title: media.title.clone(),
                        uploader: media.uploader.clone(),
                        duration: media.duration,
                        thumbnail: media.thumbnail.clone(),
                        index,
                        playlist_item: Some(index),
                        available: true,
                        media: Some(Box::new(media)),
                    });
                }
                Err(_) => continue, // Image-only carousel items are skipped.
            }
            continue;
        }
        let id = str_field(e, "id").unwrap_or_default();
        let url = str_field(e, "url")
            .filter(|u| u.starts_with("https://"))
            .or_else(|| {
                (detected.platform == Platform::Youtube && id.len() == 11)
                    .then(|| format!("https://www.youtube.com/watch?v={id}"))
            });
        let Some(url) = url else { continue };
        let available = !is_unavailable_title(&title) && crate::platform::detect(&url).is_ok();
        entries.push(PlaylistEntry {
            id,
            url,
            title,
            uploader: str_field(e, "uploader").or_else(|| str_field(e, "channel")),
            duration: f64_field(e, "duration"),
            thumbnail: pick_thumbnail(e),
            index,
            playlist_item: None,
            available,
            media: None,
        });
    }
    if entries.is_empty() {
        return Err(AppError::new(
            ErrorCode::FormatUnavailable,
            "This playlist or post has no downloadable videos.",
        ));
    }
    // A carousel with a single video is presented as that media.
    if entries.len() == 1 && entries[0].media.is_some() {
        let media = entries.remove(0).media.expect("checked");
        return Ok(Analysis::Media(media));
    }
    Ok(Analysis::Playlist(PlaylistInfo {
        id: str_field(v, "id").unwrap_or_default(),
        url: parent_url,
        title: str_field(v, "title").unwrap_or_else(|| "Playlist".into()),
        uploader: str_field(v, "uploader").or_else(|| str_field(v, "channel")),
        platform: detected.platform,
        thumbnail: pick_thumbnail(v).or_else(|| entries.iter().find_map(|e| e.thumbnail.clone())),
        entries,
    }))
}

/// Maps yt-dlp error output to a structured error with a clear message.
pub fn classify_error(stderr: &str) -> AppError {
    let error_line = stderr
        .lines()
        .rev()
        .find(|l| l.contains("ERROR:"))
        .map(|l| {
            l.split_once("ERROR:")
                .map(|(_, rest)| rest.trim())
                .unwrap_or(l)
                .to_string()
        })
        .unwrap_or_else(|| {
            stderr
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .trim()
                .to_string()
        });
    let lower = stderr.to_ascii_lowercase();
    let has = |needle: &str| lower.contains(needle);

    let (code, message) = if has("unsupported url") {
        (
            ErrorCode::UnsupportedUrl,
            "This link is not supported by the extractor.",
        )
    } else if has("drm") {
        (
            ErrorCode::DrmProtected,
            "This content is DRM protected and cannot be downloaded.",
        )
    } else if has("private video") || has("this video is private") || has("this account is private")
    {
        (ErrorCode::PrivateContent, "This content is private.")
    } else if has("sign in to confirm your age")
        || has("age-restricted")
        || has("inappropriate for some users")
    {
        (
            ErrorCode::AgeRestricted,
            "This content is age-restricted and requires an account.",
        )
    } else if has("members-only")
        || has("join this channel")
        || has("requires payment")
        || has("premium") && has("only available")
    {
        (
            ErrorCode::LoginRequired,
            "This content is only available to members or subscribers.",
        )
    } else if has("in your country") || has("geo restrict") || has("blocked it in your country") {
        (
            ErrorCode::GeoRestricted,
            "This content is not available in your region.",
        )
    } else if has("this live event will begin") || has("premieres in") {
        (
            ErrorCode::LiveNotFinished,
            "This livestream or premiere has not finished yet.",
        )
    } else if has("requested format is not available") {
        (
            ErrorCode::FormatUnavailable,
            "The selected quality is no longer available. Analyze the link again.",
        )
    } else if has("http error 429")
        || has("too many requests")
        || has("rate-limit")
        || has("rate limit")
    {
        (
            ErrorCode::RateLimited,
            "The platform is limiting requests. Wait a few minutes and try again.",
        )
    } else if has("login required")
        || has("log in")
        || has("sign in")
        || has("cookies") && has("authentication")
    {
        (
            ErrorCode::LoginRequired,
            "This content requires signing in and cannot be downloaded.",
        )
    } else if has("video unavailable")
        || has("has been removed")
        || has("does not exist")
        || has("http error 404")
        || has("not available")
        || has("no video")
    {
        (
            ErrorCode::Unavailable,
            "This content is unavailable. It may have been removed.",
        )
    } else if has("no space left") || has("not enough space") || has("errno 28") || has("disk full")
    {
        (
            ErrorCode::DiskFull,
            "There is not enough free disk space to finish the download.",
        )
    } else if has("permission denied") || has("access is denied") || has("errno 13") {
        (
            ErrorCode::PermissionDenied,
            "DarkLoad does not have permission to write the file.",
        )
    } else if has("timed out")
        || has("connection reset")
        || has("connection refused")
        || has("temporary failure in name resolution")
        || has("getaddrinfo failed")
        || has("name or service not known")
        || has("network is unreachable")
        || has("unable to download")
        || has("http error 5")
        || has("incomplete read")
        || has("connection aborted")
        || has("ssl")
        || has("remote end closed")
    {
        (
            ErrorCode::Network,
            "A network error interrupted the request. Check your connection and try again.",
        )
    } else if has("ffmpeg") && (has("not found") || has("not installed")) {
        (
            ErrorCode::DependencyMissing,
            "FFmpeg is required for this download. Install it from Settings → Dependencies.",
        )
    } else if has("postprocessing") || has("conversion failed") {
        (
            ErrorCode::ProcessFailed,
            "Media processing failed after the download finished.",
        )
    } else {
        (ErrorCode::ProcessFailed, "The download failed.")
    };
    AppError::new(code, message).with_detail(error_line)
}

/// Returns the platform name for a stored platform string.
pub fn platform_label(platform: &str) -> &'static str {
    Platform::parse(platform)
        .map(Platform::display_name)
        .unwrap_or("Unknown")
}

pub fn ensure_exists(path: &Path) -> AppResult<()> {
    if path.exists() {
        Ok(())
    } else {
        Err(AppError::new(
            ErrorCode::DependencyMissing,
            "yt-dlp is not installed. Install it from Settings → Dependencies.",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::detect;
    use serde_json::json;

    fn yt() -> DetectedUrl {
        detect("https://www.youtube.com/watch?v=aqz-KE-bpKQ").unwrap()
    }

    fn fixture_video() -> Value {
        json!({
            "id": "aqz-KE-bpKQ",
            "title": "Big Buck Bunny 60fps 4K",
            "uploader": "Blender",
            "duration": 635.0,
            "webpage_url": "https://www.youtube.com/watch?v=aqz-KE-bpKQ",
            "thumbnail": "https://i.ytimg.com/vi/aqz-KE-bpKQ/maxresdefault.jpg",
            "thumbnails": [
                {"url": "https://i.ytimg.com/vi/aqz-KE-bpKQ/default.jpg", "width": 120},
                {"url": "https://i.ytimg.com/vi/aqz-KE-bpKQ/hq720.jpg", "width": 1280},
                {"url": "https://i.ytimg.com/vi/aqz-KE-bpKQ/sd.jpg", "width": 640}
            ],
            "live_status": "not_live",
            "chapters": [{"title": "a"}, {"title": "b"}],
            "subtitles": {"en": [{"name": "English"}], "live_chat": []},
            "automatic_captions": {"en": [{}], "en-de": [{}], "fr-orig": [{}]},
            "formats": [
                {"format_id": "sb0", "ext": "mhtml", "vcodec": "none", "acodec": "none"},
                {"format_id": "140", "ext": "m4a", "vcodec": "none", "acodec": "mp4a.40.2", "abr": 129.5, "filesize": 10285000, "language_preference": -1},
                {"format_id": "251", "ext": "webm", "vcodec": "none", "acodec": "opus", "abr": 139.1, "filesize": 11040000, "language_preference": -1},
                {"format_id": "401", "ext": "mp4", "vcodec": "av01.0.13M.08", "acodec": "none", "width": 3840, "height": 2160, "fps": 60, "vbr": 18000.0, "filesize_approx": 1430000000, "dynamic_range": "SDR"},
                {"format_id": "137", "ext": "mp4", "vcodec": "avc1.640028", "acodec": "none", "width": 1920, "height": 1080, "fps": 30, "vbr": 4400.0, "filesize": 349000000, "dynamic_range": "SDR"},
                {"format_id": "18", "ext": "mp4", "vcodec": "avc1.42001E", "acodec": "mp4a.40.2", "width": 640, "height": 360, "fps": 30, "tbr": 600.0}
            ]
        })
    }

    #[test]
    fn parses_video_metadata_and_formats() {
        let Analysis::Media(m) = parse_analysis(&fixture_video(), &yt()).unwrap() else {
            panic!("expected media")
        };
        assert_eq!(m.title, "Big Buck Bunny 60fps 4K");
        assert_eq!(m.uploader.as_deref(), Some("Blender"));
        assert_eq!(m.duration, Some(635.0));
        assert_eq!(m.formats.len(), 6);
        assert_eq!(m.summary.max_height, Some(2160));
        assert_eq!(m.chapters, 2);
        assert_eq!(
            m.thumbnail.as_deref(),
            Some("https://i.ytimg.com/vi/aqz-KE-bpKQ/sd.jpg")
        );
        let f401 = m.formats.iter().find(|f| f.id == "401").unwrap();
        assert_eq!(f401.acodec, None);
        assert_eq!(f401.filesize_approx, Some(1_430_000_000));
        let langs: Vec<_> = m
            .subtitles
            .iter()
            .map(|s| (s.language.as_str(), s.automatic))
            .collect();
        assert_eq!(langs, vec![("en", false), ("en", true), ("fr-orig", true)]);
    }

    #[test]
    fn rejects_active_livestreams() {
        let mut v = fixture_video();
        v["live_status"] = json!("is_live");
        assert_eq!(
            parse_analysis(&v, &yt()).unwrap_err().code,
            ErrorCode::LiveNotFinished
        );
        v["live_status"] = json!("was_live");
        let Analysis::Media(m) = parse_analysis(&v, &yt()).unwrap() else {
            panic!()
        };
        assert!(m.was_live);
    }

    #[test]
    fn rejects_media_without_streams() {
        let v = json!({"id": "x", "title": "t", "formats": [{"format_id": "sb0", "ext": "mhtml", "vcodec": "none", "acodec": "none"}]});
        assert_eq!(
            parse_analysis(&v, &yt()).unwrap_err().code,
            ErrorCode::FormatUnavailable
        );
    }

    #[test]
    fn parses_flat_playlists() {
        let detected = detect("https://www.youtube.com/playlist?list=PLtest12345").unwrap();
        let v = json!({
            "_type": "playlist", "id": "PLtest12345", "title": "Open Movies", "uploader": "Blender",
            "entries": [
                {"id": "aqz-KE-bpKQ", "url": "https://www.youtube.com/watch?v=aqz-KE-bpKQ", "title": "Big Buck Bunny", "duration": 635.0,
                 "thumbnails": [{"url": "https://i.ytimg.com/vi/aqz-KE-bpKQ/hqdefault.jpg", "width": 480}]},
                {"id": "xxxxxxxxxxx", "url": "https://www.youtube.com/watch?v=xxxxxxxxxxx", "title": "[Private video]"},
                null,
                {"id": "eRsGyueVLvQ", "title": "Sintel"}
            ]
        });
        let Analysis::Playlist(p) = parse_analysis(&v, &detected).unwrap() else {
            panic!()
        };
        assert_eq!(p.title, "Open Movies");
        assert_eq!(p.entries.len(), 3);
        assert!(p.entries[0].available);
        assert!(!p.entries[1].available);
        assert_eq!(
            p.entries[2].url,
            "https://www.youtube.com/watch?v=eRsGyueVLvQ"
        );
        assert_eq!(p.entries[2].index, 4);
        assert_eq!(
            p.thumbnail.as_deref(),
            Some("https://i.ytimg.com/vi/aqz-KE-bpKQ/hqdefault.jpg")
        );
    }

    #[test]
    fn parses_carousels() {
        let detected = detect("https://www.instagram.com/p/C1a2B3c4D5e/").unwrap();
        let item = |id: &str| json!({"id": id, "title": "Post", "formats": [{"format_id": "dash-v", "ext": "mp4", "vcodec": "avc1", "acodec": "none", "width": 1080, "height": 1350}, {"format_id": "dash-a", "ext": "m4a", "vcodec": "none", "acodec": "mp4a.40.2", "abr": 128}]});
        let image_only = json!({"id": "img", "title": "Photo", "formats": []});
        let v = json!({"_type": "playlist", "id": "C1a2B3c4D5e", "title": "Post", "webpage_url": "https://www.instagram.com/p/C1a2B3c4D5e/",
            "entries": [item("a"), image_only, item("b")]});
        let Analysis::Playlist(p) = parse_analysis(&v, &detected).unwrap() else {
            panic!()
        };
        assert_eq!(p.entries.len(), 2);
        assert_eq!(p.entries[1].playlist_item, Some(3));
        assert!(p.entries[0].media.is_some());

        let single = json!({"_type": "playlist", "id": "C1", "entries": [item("only")]});
        assert!(
            matches!(parse_analysis(&single, &detected).unwrap(), Analysis::Media(m) if m.playlist_item == Some(1))
        );
    }

    #[test]
    fn classifies_errors() {
        let cases = [
            ("ERROR: [youtube] abc: Private video. Sign in if you've been granted access", ErrorCode::PrivateContent),
            ("ERROR: [youtube] abc: Video unavailable", ErrorCode::Unavailable),
            ("ERROR: Unsupported URL: https://example.com", ErrorCode::UnsupportedUrl),
            ("ERROR: [youtube] abc: Sign in to confirm your age.", ErrorCode::AgeRestricted),
            ("ERROR: unable to download video data: HTTP Error 503: Service Unavailable", ErrorCode::Network),
            ("ERROR: Unable to download webpage: <urlopen error [Errno 11001] getaddrinfo failed>", ErrorCode::Network),
            ("ERROR: unable to download video data: HTTP Error 429: Too Many Requests", ErrorCode::RateLimited),
            ("ERROR: [Errno 28] No space left on device", ErrorCode::DiskFull),
            ("ERROR: unable to open for writing: [Errno 13] Permission denied", ErrorCode::PermissionDenied),
            ("ERROR: [youtube] abc: Requested format is not available.", ErrorCode::FormatUnavailable),
            ("ERROR: [Instagram] abc: Requested content is not available, rate-limit reached or login required.", ErrorCode::RateLimited),
            ("ERROR: This video is DRM protected", ErrorCode::DrmProtected),
            ("ERROR: [youtube] abc: The uploader has not made this video available in your country", ErrorCode::GeoRestricted),
            ("something odd happened", ErrorCode::ProcessFailed),
        ];
        for (stderr, expected) in cases {
            let err = classify_error(stderr);
            assert_eq!(err.code, expected, "{stderr}");
            assert!(err.detail.is_some());
        }
    }

    #[test]
    fn transient_classification_is_retryable() {
        assert!(classify_error("ERROR: Read timed out.").retryable());
        assert!(!classify_error("ERROR: Private video").retryable());
    }

    #[test]
    fn base_args_never_include_user_input() {
        let tools = ToolPaths {
            yt_dlp: "yt-dlp".into(),
            ffmpeg: Some("ffmpeg".into()),
            deno: Some("deno".into()),
            cache_dir: "cache".into(),
        };
        let args = base_args(
            &tools,
            NetOptions {
                socket_timeout: 30,
                force_ipv4: true,
            },
        );
        assert!(args.contains(&OsString::from("--force-ipv4")));
        assert!(args.contains(&OsString::from("--ignore-config")));
        assert!(args.contains(&OsString::from("deno:deno")));
    }
}
