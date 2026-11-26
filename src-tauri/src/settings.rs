//! Application settings: schema, defaults and validation.
//!
//! Settings are persisted as a single JSON document in SQLite. Unknown or missing fields fall back
//! to defaults, which keeps older settings files readable after upgrades.

use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};
use crate::formats::{
    AudioCodecPreference, AudioFormat, Container, QualityMode, VideoCodecPreference,
};
use crate::security::{validate_filename_template, DuplicatePolicy};

pub const DEFAULT_TEMPLATE: &str = "%(title).180B [%(id)s].%(ext)s";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Video,
    Audio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PartialFilePolicy {
    /// Remove partially downloaded data when a download is cancelled.
    Delete,
    /// Keep partial data so the download can resume when retried.
    Keep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaylistBehavior {
    /// Show every entry with nothing selected and let the user choose.
    Select,
    /// Pre-select every available entry.
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Oled,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Comfortable,
    Compact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarBehavior {
    /// Remember the last manual state.
    Remember,
    /// Collapse automatically on narrow windows.
    Auto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    // General
    pub download_dir: String,
    pub default_media_type: MediaType,
    pub default_container: Container,
    pub default_quality_mode: QualityMode,
    pub default_audio_format: AudioFormat,
    /// `None` selects the best available audio quality.
    pub default_audio_bitrate: Option<u32>,
    pub filename_template: String,
    pub native_window_frame: bool,
    pub confirm_exit_with_active_downloads: bool,

    // Downloads
    pub max_concurrent_downloads: u8,
    /// `None` disables the speed limit. Kibibytes per second.
    pub speed_limit_kib: Option<u32>,
    pub retry_attempts: u8,
    pub socket_timeout_secs: u32,
    pub concurrent_fragments: u8,
    pub duplicate_policy: DuplicatePolicy,
    pub partial_files: PartialFilePolicy,
    pub playlist_behavior: PlaylistBehavior,
    pub resume_queue_on_startup: bool,

    // Default advanced options
    pub video_codec: VideoCodecPreference,
    pub audio_codec: AudioCodecPreference,
    pub allow_hdr: bool,
    pub embed_metadata: bool,
    pub embed_thumbnail: bool,
    pub write_thumbnail: bool,
    pub embed_chapters: bool,
    pub download_subtitles: bool,
    pub embed_subtitles: bool,
    pub subtitle_languages: Vec<String>,
    pub include_auto_subtitles: bool,

    // Appearance
    pub theme: Theme,
    pub accent_color: String,
    pub sidebar_collapsed: bool,
    pub sidebar_behavior: SidebarBehavior,
    pub density: Density,
    pub animations: bool,

    // Updates
    pub auto_check_updates: bool,

    // Privacy
    pub clipboard_monitoring: bool,
    pub notifications: bool,

    // Dependencies
    pub allow_system_binaries: bool,

    // Network
    /// Connect over IPv4 only. Avoids long stalls on networks with broken IPv6 routes.
    pub prefer_ipv4: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: String::new(),
            default_media_type: MediaType::Video,
            default_container: Container::Auto,
            default_quality_mode: QualityMode::Best,
            default_audio_format: AudioFormat::Original,
            default_audio_bitrate: None,
            filename_template: DEFAULT_TEMPLATE.to_string(),
            native_window_frame: false,
            confirm_exit_with_active_downloads: true,
            max_concurrent_downloads: 3,
            speed_limit_kib: None,
            retry_attempts: 3,
            socket_timeout_secs: 30,
            concurrent_fragments: 4,
            duplicate_policy: DuplicatePolicy::Rename,
            partial_files: PartialFilePolicy::Delete,
            playlist_behavior: PlaylistBehavior::All,
            resume_queue_on_startup: true,
            video_codec: VideoCodecPreference::Any,
            audio_codec: AudioCodecPreference::Any,
            allow_hdr: true,
            embed_metadata: true,
            embed_thumbnail: true,
            write_thumbnail: false,
            embed_chapters: true,
            download_subtitles: false,
            embed_subtitles: true,
            subtitle_languages: vec!["en".to_string()],
            include_auto_subtitles: false,
            theme: Theme::Oled,
            accent_color: "#8B5CF6".to_string(),
            sidebar_collapsed: false,
            sidebar_behavior: SidebarBehavior::Remember,
            density: Density::Comfortable,
            animations: true,
            auto_check_updates: true,
            clipboard_monitoring: false,
            notifications: true,
            allow_system_binaries: false,
            prefer_ipv4: true,
        }
    }
}

pub fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

pub fn is_language_code(value: &str) -> bool {
    // yt-dlp accepts language codes such as "en", "pt-BR", "zh-Hans" and the "all" keyword.
    (1..=16).contains(&value.len())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl Settings {
    /// Validates every field, returning a descriptive error for the first invalid value.
    pub fn validate(&self) -> AppResult<()> {
        if !(1..=8).contains(&self.max_concurrent_downloads) {
            return Err(AppError::invalid_input(
                "Concurrent downloads must be between 1 and 8.",
            ));
        }
        if self.retry_attempts > 10 {
            return Err(AppError::invalid_input(
                "Retry attempts must be between 0 and 10.",
            ));
        }
        if !(5..=600).contains(&self.socket_timeout_secs) {
            return Err(AppError::invalid_input(
                "The network timeout must be between 5 and 600 seconds.",
            ));
        }
        if !(1..=16).contains(&self.concurrent_fragments) {
            return Err(AppError::invalid_input(
                "Concurrent fragments must be between 1 and 16.",
            ));
        }
        if let Some(limit) = self.speed_limit_kib {
            if !(16..=10_000_000).contains(&limit) {
                return Err(AppError::invalid_input(
                    "The speed limit must be between 16 KiB/s and 10 GiB/s.",
                ));
            }
        }
        if let Some(bitrate) = self.default_audio_bitrate {
            if !(32..=512).contains(&bitrate) {
                return Err(AppError::invalid_input(
                    "The audio bitrate must be between 32 and 512 kbps.",
                ));
            }
        }
        validate_filename_template(&self.filename_template)?;
        if !is_hex_color(&self.accent_color) {
            return Err(AppError::invalid_input(
                "The accent color must be a hex color such as #8B5CF6.",
            ));
        }
        if self.subtitle_languages.len() > 20
            || !self.subtitle_languages.iter().all(|l| is_language_code(l))
        {
            return Err(AppError::invalid_input(
                "Subtitle languages must be language codes such as en or pt-BR.",
            ));
        }
        if self.download_dir.len() > 1024 {
            return Err(AppError::invalid_input(
                "The download folder path is too long.",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        Settings::default().validate().unwrap();
    }

    #[test]
    fn privacy_defaults() {
        let s = Settings::default();
        assert!(
            !s.clipboard_monitoring,
            "clipboard monitoring must be opt-in"
        );
        assert!(!s.allow_system_binaries);
        assert_eq!(s.theme, Theme::Oled);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let parsed: Settings =
            serde_json::from_str(r#"{"maxConcurrentDownloads":5,"unknownField":true}"#).unwrap();
        assert_eq!(parsed.max_concurrent_downloads, 5);
        assert_eq!(parsed.filename_template, DEFAULT_TEMPLATE);
    }

    #[test]
    fn round_trips_through_json() {
        let s = Settings {
            speed_limit_kib: Some(2048),
            accent_color: "#22C55E".into(),
            ..Settings::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn rejects_invalid_values() {
        type Mutation = Box<dyn Fn(&mut Settings)>;
        let cases: Vec<Mutation> = vec![
            Box::new(|s| s.max_concurrent_downloads = 0),
            Box::new(|s| s.max_concurrent_downloads = 9),
            Box::new(|s| s.retry_attempts = 11),
            Box::new(|s| s.socket_timeout_secs = 1),
            Box::new(|s| s.speed_limit_kib = Some(1)),
            Box::new(|s| s.default_audio_bitrate = Some(1000)),
            Box::new(|s| s.filename_template = "../x.%(ext)s".into()),
            Box::new(|s| s.accent_color = "purple".into()),
            Box::new(|s| s.subtitle_languages = vec!["en;rm".into()]),
            Box::new(|s| s.concurrent_fragments = 0),
        ];
        for mutate in cases {
            let mut s = Settings::default();
            mutate(&mut s);
            assert!(s.validate().is_err(), "{s:?}");
        }
    }
}
