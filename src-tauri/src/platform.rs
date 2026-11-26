//! URL validation, normalization and source-platform detection.
//!
//! Every URL entering the backend passes through [`detect`]. Only public content URLs from the
//! supported platforms are accepted; everything else is rejected before any process is spawned.

use serde::{Deserialize, Serialize};
use url::Url;

use crate::errors::{AppError, AppResult, ErrorCode};

pub const MAX_URL_LENGTH: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Youtube,
    Tiktok,
    Instagram,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Youtube => "youtube",
            Platform::Tiktok => "tiktok",
            Platform::Instagram => "instagram",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Platform::Youtube => "YouTube",
            Platform::Tiktok => "TikTok",
            Platform::Instagram => "Instagram",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "youtube" => Some(Platform::Youtube),
            "tiktok" => Some(Platform::Tiktok),
            "instagram" => Some(Platform::Instagram),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UrlKind {
    Video,
    Short,
    Live,
    Playlist,
    Reel,
    Post,
    /// A shortened link whose target is resolved by the extractor.
    Shortlink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedUrl {
    pub platform: Platform,
    pub kind: UrlKind,
    /// Canonical HTTPS URL passed to the extractor.
    pub url: String,
    pub id: Option<String>,
    /// Playlist context for YouTube watch URLs that also carry a `list` parameter.
    pub playlist_id: Option<String>,
}

fn invalid(message: &str) -> AppError {
    AppError::new(ErrorCode::InvalidUrl, message)
}

fn unsupported(message: &str) -> AppError {
    AppError::new(ErrorCode::UnsupportedUrl, message)
}

fn is_youtube_video_id(id: &str) -> bool {
    id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn is_youtube_playlist_id(id: &str) -> bool {
    (2..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn is_shortcode(id: &str) -> bool {
    (4..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn is_numeric_id(id: &str) -> bool {
    (5..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())
}

fn host_matches(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

/// Parses user input into a URL, accepting inputs that omit the scheme.
fn parse_input(input: &str) -> AppResult<Url> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(invalid("Enter a link to analyze."));
    }
    if trimmed.len() > MAX_URL_LENGTH {
        return Err(invalid("The link is too long."));
    }
    if trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(invalid("The link contains spaces or invalid characters."));
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let url =
        Url::parse(&candidate).map_err(|_| invalid("This does not look like a valid link."))?;
    if url.scheme() != "https" && url.scheme() != "http" {
        return Err(invalid("Only http and https links are supported."));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid("Links containing credentials are not accepted."));
    }
    if url.port().is_some() {
        return Err(invalid("Links with custom ports are not supported."));
    }
    Ok(url)
}

/// Validates a URL and identifies its platform and content type.
pub fn detect(input: &str) -> AppResult<DetectedUrl> {
    let url = parse_input(input)?;
    let host = url
        .host_str()
        .ok_or_else(|| invalid("The link has no host name."))?
        .to_ascii_lowercase();
    let host = host.trim_end_matches('.');

    if host == "youtu.be"
        || host_matches(host, "youtube.com")
        || host_matches(host, "youtube-nocookie.com")
    {
        return detect_youtube(&url, host);
    }
    if host_matches(host, "tiktok.com") {
        return detect_tiktok(&url, host);
    }
    if host_matches(host, "instagram.com") || host == "instagr.am" {
        return detect_instagram(&url);
    }
    Err(unsupported(
        "This website is not supported. DarkLoad supports YouTube, TikTok and Instagram links.",
    ))
}

fn path_segments(url: &Url) -> Vec<String> {
    url.path_segments()
        .map(|segments| {
            segments
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn query_param(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

fn youtube_watch(id: &str) -> String {
    format!("https://www.youtube.com/watch?v={id}")
}

fn detect_youtube(url: &Url, host: &str) -> AppResult<DetectedUrl> {
    let segments = path_segments(url);
    let first = segments.first().map(String::as_str).unwrap_or("");
    let playlist_id = query_param(url, "list").filter(|id| is_youtube_playlist_id(id));
    let video = |kind: UrlKind, id: &str, canonical: String| -> AppResult<DetectedUrl> {
        if !is_youtube_video_id(id) {
            return Err(invalid("The YouTube video ID in this link is not valid."));
        }
        Ok(DetectedUrl {
            platform: Platform::Youtube,
            kind,
            url: canonical,
            id: Some(id.to_string()),
            playlist_id: playlist_id.clone(),
        })
    };

    if host == "youtu.be" {
        let id = segments
            .first()
            .ok_or_else(|| invalid("This YouTube short link has no video ID."))?;
        return video(UrlKind::Video, id, youtube_watch(id));
    }

    match first {
        "watch" => {
            if let Some(id) = query_param(url, "v") {
                return video(UrlKind::Video, &id, youtube_watch(&id));
            }
            if let Some(list) = playlist_id.clone() {
                return Ok(playlist(list));
            }
            Err(invalid("This YouTube link has no video ID."))
        }
        "shorts" => {
            let id = segments.get(1).ok_or_else(|| invalid("This Shorts link has no video ID."))?;
            video(UrlKind::Short, id, format!("https://www.youtube.com/shorts/{id}"))
        }
        "live" => {
            let id = segments.get(1).ok_or_else(|| invalid("This livestream link has no video ID."))?;
            video(UrlKind::Live, id, youtube_watch(id))
        }
        "embed" | "v" | "e" => {
            let id = segments.get(1).ok_or_else(|| invalid("This embedded video link has no video ID."))?;
            if id == "videoseries" {
                return playlist_id.map(playlist).ok_or_else(|| invalid("This embedded playlist has no ID."));
            }
            video(UrlKind::Video, id, youtube_watch(id))
        }
        "playlist" => playlist_id.map(playlist).ok_or_else(|| invalid("This playlist link has no playlist ID.")),
        "" => Err(invalid("Open a specific YouTube video, Short or playlist and copy its link.")),
        _ => Err(unsupported(
            "Channel pages and other YouTube pages are not supported. Use a link to a video, Short, livestream or playlist.",
        )),
    }
}

fn playlist(id: String) -> DetectedUrl {
    DetectedUrl {
        platform: Platform::Youtube,
        kind: UrlKind::Playlist,
        url: format!("https://www.youtube.com/playlist?list={id}"),
        id: Some(id),
        playlist_id: None,
    }
}

fn detect_tiktok(url: &Url, host: &str) -> AppResult<DetectedUrl> {
    let segments = path_segments(url);
    let shortlink = |code: &str| -> AppResult<DetectedUrl> {
        if !is_shortcode(code) {
            return Err(invalid("This TikTok short link is not valid."));
        }
        Ok(DetectedUrl {
            platform: Platform::Tiktok,
            kind: UrlKind::Shortlink,
            url: format!("https://{host}/{}", segments.join("/")),
            id: None,
            playlist_id: None,
        })
    };

    if host == "vm.tiktok.com" || host == "vt.tiktok.com" {
        let code = segments
            .first()
            .ok_or_else(|| invalid("This TikTok short link is incomplete."))?;
        return shortlink(code);
    }

    match segments.as_slice() {
        [t, code] if t == "t" => {
            if !is_shortcode(code) {
                return Err(invalid("This TikTok short link is not valid."));
            }
            Ok(DetectedUrl {
                platform: Platform::Tiktok,
                kind: UrlKind::Shortlink,
                url: format!("https://www.tiktok.com/t/{code}"),
                id: None,
                playlist_id: None,
            })
        }
        [user, kind, id, ..] if user.starts_with('@') && (kind == "video" || kind == "photo") => {
            if !is_numeric_id(id) {
                return Err(invalid("The TikTok video ID in this link is not valid."));
            }
            let handle = &user[1..];
            if handle.is_empty()
                || !handle
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '.' || c == '_')
            {
                return Err(invalid("The TikTok username in this link is not valid."));
            }
            Ok(DetectedUrl {
                platform: Platform::Tiktok,
                kind: if kind == "video" {
                    UrlKind::Video
                } else {
                    UrlKind::Post
                },
                url: format!("https://www.tiktok.com/{user}/{kind}/{id}"),
                id: Some(id.clone()),
                playlist_id: None,
            })
        }
        [v, file] if v == "v" => {
            let id = file.trim_end_matches(".html");
            if !is_numeric_id(id) {
                return Err(invalid("The TikTok video ID in this link is not valid."));
            }
            Ok(DetectedUrl {
                platform: Platform::Tiktok,
                kind: UrlKind::Video,
                url: format!("https://www.tiktok.com/@/video/{id}"),
                id: Some(id.to_string()),
                playlist_id: None,
            })
        }
        [embed, version, id] if embed == "embed" && version == "v2" && is_numeric_id(id) => {
            Ok(DetectedUrl {
                platform: Platform::Tiktok,
                kind: UrlKind::Video,
                url: format!("https://www.tiktok.com/@/video/{id}"),
                id: Some(id.clone()),
                playlist_id: None,
            })
        }
        _ => Err(unsupported(
            "Only links to individual public TikTok videos are supported.",
        )),
    }
}

fn detect_instagram(url: &Url) -> AppResult<DetectedUrl> {
    let segments = path_segments(url);
    // Supports /reel/CODE, /reels/CODE, /p/CODE, /tv/CODE and /USERNAME/reel/CODE.
    let (kind_segment, code) = match segments.as_slice() {
        [kind, code, ..] if matches!(kind.as_str(), "reel" | "reels" | "p" | "tv") => {
            (kind.as_str(), code)
        }
        [_, kind, code, ..] if matches!(kind.as_str(), "reel" | "p" | "tv") => {
            (kind.as_str(), code)
        }
        [first, ..] if first == "stories" => {
            return Err(unsupported(
                "Instagram stories require an account and are not supported.",
            ));
        }
        _ => {
            return Err(unsupported(
                "Only links to public Instagram reels and posts are supported.",
            ))
        }
    };
    if !is_shortcode(code) {
        return Err(invalid(
            "The Instagram post code in this link is not valid.",
        ));
    }
    let (kind, path) = match kind_segment {
        "reel" | "reels" => (UrlKind::Reel, "reel"),
        "tv" => (UrlKind::Video, "tv"),
        _ => (UrlKind::Post, "p"),
    };
    Ok(DetectedUrl {
        platform: Platform::Instagram,
        kind,
        url: format!("https://www.instagram.com/{path}/{code}/"),
        id: Some(code.clone()),
        playlist_id: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(input: &str) -> DetectedUrl {
        detect(input).unwrap_or_else(|e| panic!("{input}: {e}"))
    }

    fn code(input: &str) -> ErrorCode {
        detect(input).expect_err(input).code
    }

    #[test]
    fn youtube_watch_urls() {
        let d = ok("https://www.youtube.com/watch?v=dQw4w9WgXcQ&si=tracking&feature=share");
        assert_eq!(d.platform, Platform::Youtube);
        assert_eq!(d.kind, UrlKind::Video);
        assert_eq!(d.url, "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(d.id.as_deref(), Some("dQw4w9WgXcQ"));
    }

    #[test]
    fn youtube_variants() {
        assert_eq!(
            ok("youtu.be/dQw4w9WgXcQ?t=10").url,
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
        assert_eq!(
            ok("https://m.youtube.com/watch?v=dQw4w9WgXcQ").kind,
            UrlKind::Video
        );
        assert_eq!(
            ok("https://music.youtube.com/watch?v=dQw4w9WgXcQ").kind,
            UrlKind::Video
        );
        let short = ok("https://youtube.com/shorts/abcdefghijk?feature=share");
        assert_eq!(short.kind, UrlKind::Short);
        assert_eq!(short.url, "https://www.youtube.com/shorts/abcdefghijk");
        assert_eq!(
            ok("https://www.youtube.com/live/abcdefghijk").kind,
            UrlKind::Live
        );
        assert_eq!(
            ok("https://www.youtube-nocookie.com/embed/abcdefghijk").kind,
            UrlKind::Video
        );
        assert_eq!(
            ok("HTTP://WWW.YOUTUBE.COM/watch?v=dQw4w9WgXcQ").platform,
            Platform::Youtube
        );
    }

    #[test]
    fn youtube_playlists() {
        let p = ok("https://www.youtube.com/playlist?list=PLabcdefghijklmnop");
        assert_eq!(p.kind, UrlKind::Playlist);
        assert_eq!(
            p.url,
            "https://www.youtube.com/playlist?list=PLabcdefghijklmnop"
        );
        let v = ok("https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PLabcdefghijklmnop&index=3");
        assert_eq!(v.kind, UrlKind::Video);
        assert_eq!(v.playlist_id.as_deref(), Some("PLabcdefghijklmnop"));
    }

    #[test]
    fn youtube_rejections() {
        assert_eq!(
            code("https://www.youtube.com/watch?v=short"),
            ErrorCode::InvalidUrl
        );
        assert_eq!(
            code("https://www.youtube.com/@somechannel"),
            ErrorCode::UnsupportedUrl
        );
        assert_eq!(code("https://www.youtube.com/"), ErrorCode::InvalidUrl);
    }

    #[test]
    fn tiktok_urls() {
        let v =
            ok("https://www.tiktok.com/@some.user_1/video/7234567890123456789?is_from_webapp=1");
        assert_eq!(v.platform, Platform::Tiktok);
        assert_eq!(v.kind, UrlKind::Video);
        assert_eq!(
            v.url,
            "https://www.tiktok.com/@some.user_1/video/7234567890123456789"
        );
        assert_eq!(
            ok("https://vm.tiktok.com/ZMabc123/").kind,
            UrlKind::Shortlink
        );
        assert_eq!(
            ok("https://vt.tiktok.com/ZSabc123/").url,
            "https://vt.tiktok.com/ZSabc123"
        );
        assert_eq!(
            ok("https://www.tiktok.com/t/ZTabc123/").kind,
            UrlKind::Shortlink
        );
        assert_eq!(
            ok("https://m.tiktok.com/v/7234567890123456789.html").kind,
            UrlKind::Video
        );
        assert_eq!(
            code("https://www.tiktok.com/@user"),
            ErrorCode::UnsupportedUrl
        );
        assert_eq!(
            code("https://www.tiktok.com/@user/video/notanid"),
            ErrorCode::InvalidUrl
        );
    }

    #[test]
    fn instagram_urls() {
        let r = ok("https://www.instagram.com/reel/C1a2B3c4D5e/?igsh=abc");
        assert_eq!(r.platform, Platform::Instagram);
        assert_eq!(r.kind, UrlKind::Reel);
        assert_eq!(r.url, "https://www.instagram.com/reel/C1a2B3c4D5e/");
        assert_eq!(ok("instagram.com/reels/C1a2B3c4D5e").kind, UrlKind::Reel);
        assert_eq!(
            ok("https://www.instagram.com/p/C1a2B3c4D5e/").kind,
            UrlKind::Post
        );
        assert_eq!(
            ok("https://www.instagram.com/someone/reel/C1a2B3c4D5e/").kind,
            UrlKind::Reel
        );
        assert_eq!(
            code("https://www.instagram.com/stories/someone/123/"),
            ErrorCode::UnsupportedUrl
        );
        assert_eq!(
            code("https://www.instagram.com/someone/"),
            ErrorCode::UnsupportedUrl
        );
    }

    #[test]
    fn rejects_invalid_input() {
        assert_eq!(code(""), ErrorCode::InvalidUrl);
        assert_eq!(code("   "), ErrorCode::InvalidUrl);
        assert_eq!(code("not a url"), ErrorCode::InvalidUrl);
        assert_eq!(
            code("ftp://youtube.com/watch?v=dQw4w9WgXcQ"),
            ErrorCode::InvalidUrl
        );
        assert_eq!(
            code("https://user:pass@youtube.com/watch?v=dQw4w9WgXcQ"),
            ErrorCode::InvalidUrl
        );
        assert_eq!(
            code("https://youtube.com:8080/watch?v=dQw4w9WgXcQ"),
            ErrorCode::InvalidUrl
        );
        assert_eq!(
            code("https://example.com/video.mp4"),
            ErrorCode::UnsupportedUrl
        );
        assert_eq!(
            code("https://notyoutube.com/watch?v=dQw4w9WgXcQ"),
            ErrorCode::UnsupportedUrl
        );
        assert_eq!(
            code("https://youtube.com.evil.example/watch?v=dQw4w9WgXcQ"),
            ErrorCode::UnsupportedUrl
        );
        let long = format!("https://youtube.com/watch?v={}", "a".repeat(MAX_URL_LENGTH));
        assert_eq!(code(&long), ErrorCode::InvalidUrl);
        assert_eq!(
            code("https://youtube.com/watch?v=dQw4w9WgXcQ\n--exec"),
            ErrorCode::InvalidUrl
        );
    }

    #[test]
    fn rejects_option_injection() {
        // Inputs that look like command-line flags never reach the extractor.
        assert!(detect("--exec=calc").is_err());
        assert!(detect("-o/etc/passwd").is_err());
    }
}
