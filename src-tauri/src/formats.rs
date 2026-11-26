//! Format model, quality selection and container compatibility.
//!
//! Selection works on the formats actually reported by the extractor. The planner picks the
//! highest-quality streams for the requested mode, decides whether a merge or conversion is
//! needed, and explains every trade-off instead of silently lowering quality.

use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult, ErrorCode};
use crate::security::validate_format_expression;
use crate::settings::MediaType;

// ---------------------------------------------------------------------------------------------
// Enumerations shared with settings and the frontend
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Container {
    Auto,
    Mp4,
    Mkv,
    Webm,
}

impl Container {
    pub fn ext(self) -> Option<&'static str> {
        match self {
            Container::Auto => None,
            Container::Mp4 => Some("mp4"),
            Container::Mkv => Some("mkv"),
            Container::Webm => Some("webm"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Container::Auto => "Auto",
            Container::Mp4 => "MP4",
            Container::Mkv => "MKV",
            Container::Webm => "WebM",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QualityMode {
    /// Highest available source quality.
    Best,
    /// A specific resolution chosen from the analyzed formats.
    Exact,
    /// H.264 + AAC in MP4 for the widest device compatibility.
    Compatible,
    /// A user-supplied yt-dlp format expression.
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodecPreference {
    Any,
    Av1,
    Vp9,
    H265,
    H264,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioCodecPreference {
    Any,
    Opus,
    Aac,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    Original,
    Mp3,
    M4a,
    Aac,
    Flac,
    Wav,
    Opus,
    Ogg,
}

impl AudioFormat {
    /// Value passed to yt-dlp `--audio-format`.
    pub fn ytdlp_name(self) -> &'static str {
        match self {
            AudioFormat::Original => "best",
            AudioFormat::Mp3 => "mp3",
            AudioFormat::M4a => "m4a",
            AudioFormat::Aac => "aac",
            AudioFormat::Flac => "flac",
            AudioFormat::Wav => "wav",
            AudioFormat::Opus => "opus",
            AudioFormat::Ogg => "vorbis",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AudioFormat::Original => "Original",
            AudioFormat::Mp3 => "MP3",
            AudioFormat::M4a => "M4A",
            AudioFormat::Aac => "AAC",
            AudioFormat::Flac => "FLAC",
            AudioFormat::Wav => "WAV",
            AudioFormat::Opus => "Opus",
            AudioFormat::Ogg => "OGG Vorbis",
        }
    }

    pub fn is_lossless(self) -> bool {
        matches!(self, AudioFormat::Flac | AudioFormat::Wav)
    }

    /// The codec that can be copied into this format without re-encoding.
    fn native_codec(self) -> Option<AudioCodec> {
        match self {
            AudioFormat::Original => None,
            AudioFormat::Mp3 => Some(AudioCodec::Mp3),
            AudioFormat::M4a | AudioFormat::Aac => Some(AudioCodec::Aac),
            AudioFormat::Flac => Some(AudioCodec::Flac),
            AudioFormat::Wav => None,
            AudioFormat::Opus => Some(AudioCodec::Opus),
            AudioFormat::Ogg => Some(AudioCodec::Vorbis),
        }
    }

    fn supports_thumbnail(self) -> bool {
        !matches!(self, AudioFormat::Wav | AudioFormat::Aac)
    }
}

// ---------------------------------------------------------------------------------------------
// Codecs
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    H264,
    H265,
    Vp9,
    Av1,
    Vp8,
    Other,
}

impl VideoCodec {
    pub fn from_codec_string(codec: &str) -> Self {
        let c = codec.to_ascii_lowercase();
        if c.starts_with("avc") || c.starts_with("h264") {
            VideoCodec::H264
        } else if c.starts_with("hvc")
            || c.starts_with("hev")
            || c.starts_with("h265")
            || c.starts_with("hevc")
        {
            VideoCodec::H265
        } else if c.starts_with("vp09") || c.starts_with("vp9") {
            VideoCodec::Vp9
        } else if c.starts_with("av01") || c == "av1" {
            VideoCodec::Av1
        } else if c.starts_with("vp8") {
            VideoCodec::Vp8
        } else {
            VideoCodec::Other
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            VideoCodec::H264 => "H.264",
            VideoCodec::H265 => "H.265",
            VideoCodec::Vp9 => "VP9",
            VideoCodec::Av1 => "AV1",
            VideoCodec::Vp8 => "VP8",
            VideoCodec::Other => "Other",
        }
    }

    /// Relative compression efficiency, used only to break ties at equal resolution and frame rate.
    fn efficiency(self) -> u8 {
        match self {
            VideoCodec::Av1 => 5,
            VideoCodec::Vp9 => 4,
            VideoCodec::H265 => 3,
            VideoCodec::H264 => 2,
            VideoCodec::Vp8 => 1,
            VideoCodec::Other => 0,
        }
    }

    fn matches(self, pref: VideoCodecPreference) -> bool {
        matches!(
            (pref, self),
            (VideoCodecPreference::Av1, VideoCodec::Av1)
                | (VideoCodecPreference::Vp9, VideoCodec::Vp9)
                | (VideoCodecPreference::H265, VideoCodec::H265)
                | (VideoCodecPreference::H264, VideoCodec::H264)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioCodec {
    Aac,
    Opus,
    Vorbis,
    Mp3,
    Flac,
    Ac3,
    Other,
}

impl AudioCodec {
    pub fn from_codec_string(codec: &str) -> Self {
        let c = codec.to_ascii_lowercase();
        if c.starts_with("mp4a") || c.starts_with("aac") {
            AudioCodec::Aac
        } else if c.starts_with("opus") {
            AudioCodec::Opus
        } else if c.starts_with("vorbis") {
            AudioCodec::Vorbis
        } else if c.starts_with("mp3") || c == "mp4a.40.34" {
            AudioCodec::Mp3
        } else if c.starts_with("flac") {
            AudioCodec::Flac
        } else if c.starts_with("ac-3")
            || c.starts_with("ac3")
            || c.starts_with("ec-3")
            || c.starts_with("eac3")
        {
            AudioCodec::Ac3
        } else {
            AudioCodec::Other
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AudioCodec::Aac => "AAC",
            AudioCodec::Opus => "Opus",
            AudioCodec::Vorbis => "Vorbis",
            AudioCodec::Mp3 => "MP3",
            AudioCodec::Flac => "FLAC",
            AudioCodec::Ac3 => "Dolby Digital",
            AudioCodec::Other => "Other",
        }
    }

    fn matches(self, pref: AudioCodecPreference) -> bool {
        matches!(
            (pref, self),
            (AudioCodecPreference::Opus, AudioCodec::Opus)
                | (AudioCodecPreference::Aac, AudioCodec::Aac)
        )
    }
}

/// Whether `container` can hold the given streams without re-encoding.
pub fn container_supports(
    container: Container,
    video: Option<VideoCodec>,
    audio: Option<AudioCodec>,
) -> bool {
    let video_ok = match (container, video) {
        (_, None) | (Container::Auto | Container::Mkv, _) => true,
        (Container::Mp4, Some(v)) => matches!(
            v,
            VideoCodec::H264 | VideoCodec::H265 | VideoCodec::Av1 | VideoCodec::Vp9
        ),
        (Container::Webm, Some(v)) => {
            matches!(v, VideoCodec::Vp9 | VideoCodec::Vp8 | VideoCodec::Av1)
        }
    };
    let audio_ok = match (container, audio) {
        (_, None) | (Container::Auto | Container::Mkv, _) => true,
        (Container::Mp4, Some(a)) => matches!(
            a,
            AudioCodec::Aac
                | AudioCodec::Mp3
                | AudioCodec::Opus
                | AudioCodec::Flac
                | AudioCodec::Ac3
        ),
        (Container::Webm, Some(a)) => matches!(a, AudioCodec::Opus | AudioCodec::Vorbis),
    };
    video_ok && audio_ok
}

/// The container that natively fits the given streams.
pub fn native_container(video: Option<VideoCodec>, audio: Option<AudioCodec>) -> Container {
    let mp4_video = matches!(
        video,
        None | Some(VideoCodec::H264 | VideoCodec::H265 | VideoCodec::Av1)
    );
    let mp4_audio = matches!(
        audio,
        None | Some(AudioCodec::Aac | AudioCodec::Mp3 | AudioCodec::Ac3)
    );
    let webm_video = matches!(
        video,
        None | Some(VideoCodec::Vp9 | VideoCodec::Vp8 | VideoCodec::Av1)
    );
    let webm_audio = matches!(audio, None | Some(AudioCodec::Opus | AudioCodec::Vorbis));
    if mp4_video && mp4_audio {
        Container::Mp4
    } else if webm_video && webm_audio {
        Container::Webm
    } else {
        Container::Mkv
    }
}

// ---------------------------------------------------------------------------------------------
// Format model
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaFormat {
    pub id: String,
    pub ext: String,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub tbr: Option<f64>,
    pub vbr: Option<f64>,
    pub abr: Option<f64>,
    pub asr: Option<u32>,
    pub audio_channels: Option<u32>,
    pub filesize: Option<u64>,
    pub filesize_approx: Option<u64>,
    pub dynamic_range: Option<String>,
    pub protocol: Option<String>,
    pub note: Option<String>,
    pub language: Option<String>,
    pub language_preference: Option<i32>,
}

impl MediaFormat {
    pub fn has_video(&self) -> bool {
        self.vcodec.is_some()
            || (self.height.is_some() && self.acodec.is_none() && self.abr.is_none())
    }

    pub fn has_audio(&self) -> bool {
        self.acodec.is_some()
    }

    pub fn video_codec(&self) -> Option<VideoCodec> {
        if !self.has_video() {
            return None;
        }
        Some(
            self.vcodec
                .as_deref()
                .map(VideoCodec::from_codec_string)
                .unwrap_or(VideoCodec::Other),
        )
    }

    pub fn audio_codec(&self) -> Option<AudioCodec> {
        self.acodec.as_deref().map(AudioCodec::from_codec_string)
    }

    pub fn is_hdr(&self) -> bool {
        matches!(self.dynamic_range.as_deref(), Some(r) if !r.eq_ignore_ascii_case("sdr"))
    }

    /// Resolution as conventionally labelled: the shorter side, so vertical 1080×1920 is "1080p".
    pub fn resolution_key(&self) -> Option<u32> {
        match (self.width, self.height) {
            (Some(w), Some(h)) if w > 0 && h > 0 => Some(w.min(h)),
            (_, Some(h)) => Some(h),
            _ => None,
        }
    }

    pub fn rounded_fps(&self) -> u32 {
        self.fps.map(|f| f.round().max(0.0) as u32).unwrap_or(0)
    }

    pub fn size(&self) -> Option<(u64, bool)> {
        self.filesize
            .map(|s| (s, false))
            .or(self.filesize_approx.map(|s| (s, true)))
    }

    fn audio_bitrate(&self) -> f64 {
        self.abr
            .or(if self.has_video() { None } else { self.tbr })
            .unwrap_or(0.0)
    }

    fn video_bitrate(&self) -> f64 {
        self.vbr.or(self.tbr).unwrap_or(0.0)
    }

    fn is_drc(&self) -> bool {
        self.id.ends_with("-drc")
            || self
                .note
                .as_deref()
                .is_some_and(|n| n.to_ascii_lowercase().contains("drc"))
    }

    /// Storyboards and image-only entries are never downloadable media.
    fn is_media(&self) -> bool {
        self.ext != "mhtml" && (self.has_video() || self.has_audio())
    }
}

// ---------------------------------------------------------------------------------------------
// Summary for the UI
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolutionOption {
    pub height: u32,
    pub label: String,
    pub frame_rates: Vec<u32>,
    pub hdr: bool,
    pub codecs: Vec<VideoCodec>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatSummary {
    pub resolutions: Vec<ResolutionOption>,
    pub video_codecs: Vec<VideoCodec>,
    pub audio_codecs: Vec<AudioCodec>,
    pub audio_bitrates: Vec<u32>,
    pub has_video: bool,
    pub has_audio: bool,
    pub has_hdr: bool,
    pub max_height: Option<u32>,
    pub max_fps: Option<u32>,
}

pub fn resolution_label(height: u32) -> String {
    match height {
        h if h >= 4320 => format!("8K · {h}p"),
        h if h >= 2160 => format!("4K · {h}p"),
        h if h >= 1440 => format!("QHD · {h}p"),
        h if h >= 1080 => format!("Full HD · {h}p"),
        h if h >= 720 => format!("HD · {h}p"),
        h => format!("{h}p"),
    }
}

fn push_unique<T: PartialEq>(list: &mut Vec<T>, value: T) {
    if !list.contains(&value) {
        list.push(value);
    }
}

pub fn summarize(formats: &[MediaFormat]) -> FormatSummary {
    let mut summary = FormatSummary::default();
    for f in formats.iter().filter(|f| f.is_media()) {
        if let Some(vc) = f.video_codec() {
            summary.has_video = true;
            push_unique(&mut summary.video_codecs, vc);
            if f.is_hdr() {
                summary.has_hdr = true;
            }
            if let Some(h) = f.resolution_key() {
                let option = match summary.resolutions.iter_mut().find(|r| r.height == h) {
                    Some(o) => o,
                    None => {
                        summary.resolutions.push(ResolutionOption {
                            height: h,
                            label: resolution_label(h),
                            frame_rates: Vec::new(),
                            hdr: false,
                            codecs: Vec::new(),
                        });
                        summary.resolutions.last_mut().expect("just pushed")
                    }
                };
                if f.rounded_fps() > 0 {
                    push_unique(&mut option.frame_rates, f.rounded_fps());
                }
                option.hdr |= f.is_hdr();
                push_unique(&mut option.codecs, vc);
            }
        }
        if let Some(ac) = f.audio_codec() {
            summary.has_audio = true;
            push_unique(&mut summary.audio_codecs, ac);
            let abr = f.audio_bitrate().round() as u32;
            if abr > 0 && !f.has_video() {
                push_unique(&mut summary.audio_bitrates, abr);
            }
        }
    }
    summary
        .resolutions
        .sort_by_key(|r| std::cmp::Reverse(r.height));
    for r in &mut summary.resolutions {
        r.frame_rates.sort_unstable_by(|a, b| b.cmp(a));
        r.codecs.sort_by_key(|c| std::cmp::Reverse(c.efficiency()));
    }
    summary
        .video_codecs
        .sort_by_key(|c| std::cmp::Reverse(c.efficiency()));
    summary.audio_bitrates.sort_unstable_by(|a, b| b.cmp(a));
    summary.max_height = summary.resolutions.first().map(|r| r.height);
    summary.max_fps = summary
        .resolutions
        .iter()
        .flat_map(|r| r.frame_rates.iter().copied())
        .max();
    summary
}

// ---------------------------------------------------------------------------------------------
// Download options and selection plans
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DownloadOptions {
    pub media_type: MediaType,
    pub quality_mode: QualityMode,
    pub target_height: Option<u32>,
    pub target_fps: Option<u32>,
    pub custom_format: Option<String>,
    pub container: Container,
    pub video_codec: VideoCodecPreference,
    pub audio_codec: AudioCodecPreference,
    pub allow_hdr: bool,
    pub audio_format: AudioFormat,
    /// Target bitrate in kbps for lossy conversions. `None` keeps the best available quality.
    pub audio_bitrate: Option<u32>,
    pub embed_metadata: bool,
    pub embed_thumbnail: bool,
    pub write_thumbnail: bool,
    pub embed_chapters: bool,
    pub download_subtitles: bool,
    pub embed_subtitles: bool,
    pub subtitle_languages: Vec<String>,
    pub include_auto_subtitles: bool,
    pub filename_template: Option<String>,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            media_type: MediaType::Video,
            quality_mode: QualityMode::Best,
            target_height: None,
            target_fps: None,
            custom_format: None,
            container: Container::Auto,
            video_codec: VideoCodecPreference::Any,
            audio_codec: AudioCodecPreference::Any,
            allow_hdr: true,
            audio_format: AudioFormat::Original,
            audio_bitrate: None,
            embed_metadata: true,
            embed_thumbnail: true,
            write_thumbnail: false,
            embed_chapters: true,
            download_subtitles: false,
            embed_subtitles: true,
            subtitle_languages: vec!["en".into()],
            include_auto_subtitles: false,
            filename_template: None,
        }
    }
}

impl DownloadOptions {
    pub fn validate(&self) -> AppResult<()> {
        if self.quality_mode == QualityMode::Custom {
            validate_format_expression(self.custom_format.as_deref().unwrap_or(""))?;
        }
        if self.quality_mode == QualityMode::Exact
            && self.media_type == MediaType::Video
            && self.target_height.is_none()
        {
            return Err(AppError::invalid_input(
                "Choose a resolution for exact quality mode.",
            ));
        }
        if let Some(h) = self.target_height {
            if !(64..=8640).contains(&h) {
                return Err(AppError::invalid_input(
                    "The requested resolution is out of range.",
                ));
            }
        }
        if let Some(b) = self.audio_bitrate {
            if !(32..=512).contains(&b) {
                return Err(AppError::invalid_input(
                    "The audio bitrate must be between 32 and 512 kbps.",
                ));
            }
        }
        if let Some(t) = &self.filename_template {
            crate::security::validate_filename_template(t)?;
        }
        if self.subtitle_languages.len() > 20
            || !self
                .subtitle_languages
                .iter()
                .all(|l| crate::settings::is_language_code(l))
        {
            return Err(AppError::invalid_input(
                "Subtitle languages must be language codes such as en or pt-BR.",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteLevel {
    Info,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanNote {
    pub level: NoteLevel,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerConflict {
    pub requested: Container,
    pub suggested: Container,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionPlan {
    pub video: Option<MediaFormat>,
    pub audio: Option<MediaFormat>,
    /// yt-dlp `-f` expression.
    pub format_selector: String,
    /// yt-dlp `-S` expression.
    pub format_sort: Option<String>,
    /// Container passed to `--merge-output-format` / `--remux-video`.
    pub output_container: Option<Container>,
    pub requires_merge: bool,
    /// Audio conversion target, when audio is extracted.
    pub audio_conversion: Option<AudioFormat>,
    /// Whether extracted audio is re-encoded (as opposed to copied).
    pub transcodes_audio: bool,
    pub estimated_size: Option<u64>,
    pub size_is_estimate: bool,
    pub quality_label: String,
    pub notes: Vec<PlanNote>,
    pub container_conflict: Option<ContainerConflict>,
    pub embed_thumbnail: bool,
    pub embed_subtitles: bool,
}

impl SelectionPlan {
    fn note(&mut self, level: NoteLevel, message: impl Into<String>) {
        self.notes.push(PlanNote {
            level,
            message: message.into(),
        });
    }

    fn empty(selector: String) -> Self {
        Self {
            video: None,
            audio: None,
            format_selector: selector,
            format_sort: None,
            output_container: None,
            requires_merge: false,
            audio_conversion: None,
            transcodes_audio: false,
            estimated_size: None,
            size_is_estimate: false,
            quality_label: String::new(),
            notes: Vec::new(),
            container_conflict: None,
            embed_thumbnail: false,
            embed_subtitles: false,
        }
    }
}

fn video_rank(
    f: &MediaFormat,
    pref: VideoCodecPreference,
) -> (u32, u32, bool, bool, bool, u8, u64) {
    let vc = f.video_codec().unwrap_or(VideoCodec::Other);
    (
        f.resolution_key().unwrap_or(0),
        f.rounded_fps(),
        f.is_hdr(),
        vc.matches(pref),
        // Prefer video-only streams (merged with the best audio) over pre-muxed ones.
        !f.has_audio(),
        vc.efficiency(),
        (f.video_bitrate() * 1000.0) as u64,
    )
}

fn audio_rank(
    f: &MediaFormat,
    pref: AudioCodecPreference,
    wanted: Option<AudioCodec>,
) -> (i32, bool, bool, bool, bool, u64) {
    let ac = f.audio_codec().unwrap_or(AudioCodec::Other);
    (
        f.language_preference.unwrap_or(0).max(-1),
        !f.is_drc(),
        wanted.is_some_and(|w| w == ac),
        ac.matches(pref),
        !f.has_video(),
        (f.audio_bitrate() * 1000.0) as u64,
    )
}

fn best_by<'a, K: Ord>(
    items: impl Iterator<Item = &'a MediaFormat>,
    key: impl Fn(&MediaFormat) -> K,
) -> Option<&'a MediaFormat> {
    items.max_by(|a, b| key(a).cmp(&key(b)))
}

fn describe_video(f: &MediaFormat) -> String {
    let mut label = match f.resolution_key() {
        Some(h) => format!("{h}p"),
        None => "Video".to_string(),
    };
    if f.rounded_fps() > 30 {
        label.push_str(&f.rounded_fps().to_string());
    }
    if f.is_hdr() {
        label.push_str(&format!(" {}", f.dynamic_range.as_deref().unwrap_or("HDR")));
    }
    if let Some(vc) = f.video_codec() {
        label.push_str(&format!(" · {}", vc.label()));
    }
    label
}

fn describe_audio(f: &MediaFormat) -> String {
    let codec = f.audio_codec().map(AudioCodec::label).unwrap_or("Audio");
    let abr = f.audio_bitrate().round() as u32;
    if abr > 0 {
        format!("{codec} {abr} kbps")
    } else {
        codec.to_string()
    }
}

fn sum_sizes(parts: &[Option<&MediaFormat>]) -> (Option<u64>, bool) {
    let mut total = 0u64;
    let mut approx = false;
    let mut any = false;
    for f in parts.iter().flatten() {
        match f.size() {
            Some((s, a)) => {
                total += s;
                approx |= a;
                any = true;
            }
            None => return (None, false),
        }
    }
    if any {
        (Some(total), approx)
    } else {
        (None, false)
    }
}

/// Builds a format expression that works without analyzed formats (used as a fallback).
pub fn generic_selector(opts: &DownloadOptions) -> (String, Option<String>) {
    match opts.media_type {
        MediaType::Audio => {
            let selector = match opts.audio_format {
                AudioFormat::M4a | AudioFormat::Aac => "ba[acodec^=mp4a]/ba/b",
                AudioFormat::Opus => "ba[acodec=opus]/ba/b",
                AudioFormat::Ogg => "ba[acodec=vorbis]/ba/b",
                _ => "ba/b",
            };
            (selector.to_string(), None)
        }
        MediaType::Video => {
            let hdr = if opts.allow_hdr {
                ""
            } else {
                "[dynamic_range=?SDR]"
            };
            let codec_sort = match opts.video_codec {
                VideoCodecPreference::Any => None,
                VideoCodecPreference::Av1 => Some("vcodec:av01"),
                VideoCodecPreference::Vp9 => Some("vcodec:vp9"),
                VideoCodecPreference::H265 => Some("vcodec:h265"),
                VideoCodecPreference::H264 => Some("vcodec:h264"),
            };
            match opts.quality_mode {
                QualityMode::Compatible => (
                    "bv*[vcodec^=avc1]+ba[acodec^=mp4a]/b[vcodec^=avc1][acodec^=mp4a]/bv*+ba/b"
                        .to_string(),
                    Some("vcodec:h264,res,fps,acodec:aac".to_string()),
                ),
                QualityMode::Exact => {
                    let h = opts.target_height.unwrap_or(1080);
                    (
                        format!("bv*[height={h}]{hdr}+ba/bv*[width={h}]{hdr}+ba/b[height={h}]/b[width={h}]"),
                        codec_sort.map(|c| format!("res,fps,{c}")),
                    )
                }
                QualityMode::Custom => (
                    opts.custom_format
                        .clone()
                        .unwrap_or_else(|| "bv*+ba/b".into()),
                    None,
                ),
                QualityMode::Best => (
                    format!("bv*{hdr}+ba/b{hdr}/bv*+ba/b"),
                    codec_sort.map(|c| format!("res,fps,{c}")),
                ),
            }
        }
    }
}

/// Plans a download from the formats offered by the source.
pub fn plan(
    formats: &[MediaFormat],
    duration: Option<f64>,
    opts: &DownloadOptions,
) -> AppResult<SelectionPlan> {
    opts.validate()?;
    let media: Vec<&MediaFormat> = formats.iter().filter(|f| f.is_media()).collect();
    if opts.quality_mode == QualityMode::Custom {
        return Ok(plan_custom(&media, opts));
    }
    if media.is_empty() {
        let (selector, sort) = generic_selector(opts);
        let mut plan = SelectionPlan::empty(selector);
        plan.format_sort = sort;
        plan.quality_label = "Best available".into();
        if opts.media_type == MediaType::Audio {
            plan.audio_conversion = Some(opts.audio_format);
            plan.embed_thumbnail = opts.embed_thumbnail && opts.audio_format.supports_thumbnail();
        } else {
            plan.output_container = opts.container.ext().map(|_| opts.container);
            plan.embed_thumbnail = opts.embed_thumbnail && opts.container != Container::Webm;
            plan.embed_subtitles = opts.download_subtitles && opts.embed_subtitles;
        }
        return Ok(plan);
    }
    match opts.media_type {
        MediaType::Video => plan_video(&media, opts),
        MediaType::Audio => plan_audio(&media, duration, opts),
    }
}

fn plan_custom(media: &[&MediaFormat], opts: &DownloadOptions) -> SelectionPlan {
    let expr = opts
        .custom_format
        .clone()
        .unwrap_or_default()
        .trim()
        .to_string();
    let mut plan = SelectionPlan::empty(expr.clone());
    // Resolve simple "id" or "id+id" expressions so the UI can show what will be downloaded.
    let ids: Vec<&str> = expr.split('+').collect();
    if ids.len() <= 2 && ids.iter().all(|id| media.iter().any(|f| f.id == *id)) {
        for id in &ids {
            let f = media.iter().find(|f| f.id == *id).copied().cloned();
            if let Some(f) = f {
                if f.has_video() && plan.video.is_none() {
                    plan.video = Some(f);
                } else if plan.audio.is_none() {
                    plan.audio = Some(f);
                }
            }
        }
        let (size, approx) = sum_sizes(&[plan.video.as_ref(), plan.audio.as_ref()]);
        plan.estimated_size = size;
        plan.size_is_estimate = approx;
    }
    plan.requires_merge = expr.contains('+');
    plan.quality_label = format!("Custom: {expr}");
    match opts.media_type {
        MediaType::Audio => {
            plan.audio_conversion = Some(opts.audio_format);
            plan.transcodes_audio = opts.audio_format != AudioFormat::Original;
            plan.embed_thumbnail = opts.embed_thumbnail && opts.audio_format.supports_thumbnail();
        }
        MediaType::Video => {
            plan.output_container = opts.container.ext().map(|_| opts.container);
            plan.embed_thumbnail = opts.embed_thumbnail && opts.container != Container::Webm;
            plan.embed_subtitles = opts.download_subtitles && opts.embed_subtitles;
        }
    }
    plan.note(
        NoteLevel::Info,
        "Custom format expressions are passed to the extractor as entered.",
    );
    plan
}

fn plan_video(media: &[&MediaFormat], opts: &DownloadOptions) -> AppResult<SelectionPlan> {
    let all_video: Vec<&MediaFormat> = media.iter().copied().filter(|f| f.has_video()).collect();
    if all_video.is_empty() {
        return Err(AppError::new(
            ErrorCode::FormatUnavailable,
            "This source does not offer a video stream. Try the Audio mode.",
        ));
    }
    let mut notes: Vec<PlanNote> = Vec::new();
    let mut candidates: Vec<&MediaFormat> = all_video.clone();
    if !opts.allow_hdr {
        let sdr: Vec<&MediaFormat> = candidates.iter().copied().filter(|f| !f.is_hdr()).collect();
        if sdr.is_empty() {
            notes.push(PlanNote {
                level: NoteLevel::Warning,
                message: "Only HDR streams are available, so HDR is kept.".into(),
            });
        } else {
            candidates = sdr;
        }
    }

    let video: &MediaFormat = match opts.quality_mode {
        QualityMode::Best => best_by(candidates.iter().copied(), |f| {
            video_rank(f, opts.video_codec)
        })
        .expect("non-empty"),
        QualityMode::Exact => {
            let h = opts.target_height.unwrap_or(0);
            let at_height = candidates
                .iter()
                .copied()
                .filter(|f| f.resolution_key() == Some(h));
            let chosen = match opts.target_fps {
                Some(fps) => best_by(at_height.filter(|f| f.rounded_fps() == fps), |f| {
                    video_rank(f, opts.video_codec)
                }),
                None => best_by(at_height, |f| video_rank(f, opts.video_codec)),
            };
            chosen.ok_or_else(|| {
                AppError::new(ErrorCode::FormatUnavailable, format!("{h}p is not available for this media. Choose one of the listed resolutions."))
            })?
        }
        QualityMode::Compatible => {
            let h264 = candidates
                .iter()
                .copied()
                .filter(|f| f.video_codec() == Some(VideoCodec::H264));
            match best_by(h264, |f| video_rank(f, VideoCodecPreference::H264)) {
                Some(f) => f,
                None => {
                    notes.push(PlanNote {
                        level: NoteLevel::Warning,
                        message: "No H.264 stream is available. The best available stream is used instead.".into(),
                    });
                    best_by(candidates.iter().copied(), |f| {
                        video_rank(f, VideoCodecPreference::Any)
                    })
                    .expect("non-empty")
                }
            }
        }
        QualityMode::Custom => unreachable!("handled by plan_custom"),
    };

    let vc = video.video_codec();
    let mut audio: Option<&MediaFormat> = None;
    if !video.has_audio() {
        let audio_only: Vec<&MediaFormat> = media
            .iter()
            .copied()
            .filter(|f| f.has_audio() && !f.has_video())
            .collect();
        audio = match opts.quality_mode {
            QualityMode::Compatible => best_by(
                audio_only
                    .iter()
                    .copied()
                    .filter(|f| f.audio_codec() == Some(AudioCodec::Aac)),
                |f| audio_rank(f, AudioCodecPreference::Aac, Some(AudioCodec::Aac)),
            )
            .or_else(|| {
                best_by(audio_only.iter().copied(), |f| {
                    audio_rank(f, AudioCodecPreference::Aac, None)
                })
            }),
            _ => {
                // With an explicit container, prefer audio that fits it when quality is otherwise equal.
                let fits =
                    |f: &MediaFormat| container_supports(opts.container, vc, f.audio_codec());
                best_by(audio_only.iter().copied(), |f| {
                    (audio_rank(f, opts.audio_codec, None), fits(f))
                })
            }
        };
        if audio.is_none() {
            notes.push(PlanNote {
                level: NoteLevel::Warning,
                message: "No separate audio stream is available; the video will be silent.".into(),
            });
        }
    }
    let ac = audio
        .and_then(|a| a.audio_codec())
        .or(if video.has_audio() {
            video.audio_codec()
        } else {
            None
        });

    let requested =
        if opts.quality_mode == QualityMode::Compatible && opts.container == Container::Auto {
            Container::Mp4
        } else {
            opts.container
        };
    // The reported container of an already bundled stream can carry both tracks, even
    // when the extractor lacks enough codec metadata to derive a container.
    let source_container =
        if audio.is_none() && (vc == Some(VideoCodec::Other) || ac == Some(AudioCodec::Other)) {
            match video.ext.as_str() {
                "mp4" => Some(Container::Mp4),
                "mkv" => Some(Container::Mkv),
                "webm" => Some(Container::Webm),
                _ => None,
            }
        } else {
            None
        };
    let mut conflict = None;
    let output_container = match requested {
        Container::Auto => source_container.unwrap_or_else(|| native_container(vc, ac)),
        c if source_container == Some(c) || container_supports(c, vc, ac) => c,
        c => {
            let suggested = Container::Mkv;
            let reason = format!(
                "{} cannot store {}{} without re-encoding. {} keeps the original streams.",
                c.label(),
                vc.map(VideoCodec::label).unwrap_or("this video"),
                ac.map(|a| format!(" + {}", a.label())).unwrap_or_default(),
                suggested.label()
            );
            conflict = Some(ContainerConflict {
                requested: c,
                suggested,
                reason,
            });
            suggested
        }
    };

    if output_container == Container::Mp4 && matches!(vc, Some(VideoCodec::Vp9))
        || output_container == Container::Mp4 && ac == Some(AudioCodec::Opus)
    {
        notes.push(PlanNote {
            level: NoteLevel::Info,
            message: "VP9 or Opus inside MP4 is preserved without quality loss, but some older players cannot play it.".into(),
        });
    }
    if output_container == Container::Mp4 && vc == Some(VideoCodec::Av1) {
        notes.push(PlanNote {
            level: NoteLevel::Info,
            message: "AV1 playback requires a recent player or hardware decoder.".into(),
        });
    }
    if video.is_hdr() {
        notes.push(PlanNote {
            level: NoteLevel::Info,
            message:
                "HDR is preserved. SDR displays may show washed-out colors without tone mapping."
                    .into(),
        });
    }

    let selector = match audio {
        Some(a) => format!("{}+{}", video.id, a.id),
        None => video.id.clone(),
    };
    let (generic, _) = generic_selector(opts);
    let mut plan = SelectionPlan::empty(format!("{selector}/{generic}"));
    plan.requires_merge = audio.is_some();
    plan.output_container = Some(output_container);
    let (size, approx) = sum_sizes(&[Some(video), audio]);
    plan.estimated_size = size;
    plan.size_is_estimate = approx;
    plan.quality_label = match audio {
        Some(a) => format!("{} + {}", describe_video(video), describe_audio(a)),
        None => describe_video(video),
    };
    plan.video = Some(video.clone());
    plan.audio = audio.cloned();
    plan.container_conflict = conflict;
    plan.embed_thumbnail = opts.embed_thumbnail && output_container != Container::Webm;
    if opts.embed_thumbnail && output_container == Container::Webm {
        notes.push(PlanNote {
            level: NoteLevel::Info,
            message: "WebM files cannot hold cover art, so the thumbnail is not embedded.".into(),
        });
    }
    plan.embed_subtitles = opts.download_subtitles && opts.embed_subtitles;
    plan.notes = notes;
    Ok(plan)
}

fn plan_audio(
    media: &[&MediaFormat],
    duration: Option<f64>,
    opts: &DownloadOptions,
) -> AppResult<SelectionPlan> {
    let audio_streams: Vec<&MediaFormat> =
        media.iter().copied().filter(|f| f.has_audio()).collect();
    if audio_streams.is_empty() {
        return Err(AppError::new(
            ErrorCode::FormatUnavailable,
            "This source does not offer an audio stream.",
        ));
    }
    let wanted = opts.audio_format.native_codec();
    let audio_only: Vec<&MediaFormat> = audio_streams
        .iter()
        .copied()
        .filter(|f| !f.has_video())
        .collect();
    let pool = if audio_only.is_empty() {
        &audio_streams
    } else {
        &audio_only
    };

    let best_overall = best_by(pool.iter().copied(), |f| {
        audio_rank(f, opts.audio_codec, None)
    })
    .expect("non-empty");
    // Prefer a stream that can be copied into the target format when it is not meaningfully worse.
    let source = match wanted.and_then(|w| {
        best_by(
            pool.iter().copied().filter(|f| f.audio_codec() == Some(w)),
            |f| audio_rank(f, opts.audio_codec, Some(w)),
        )
    }) {
        Some(native) if native.audio_bitrate() >= best_overall.audio_bitrate() * 0.75 => native,
        _ => best_overall,
    };
    let source_codec = source.audio_codec();
    let copies = match opts.audio_format {
        AudioFormat::Original => true,
        f => f.native_codec().is_some() && f.native_codec() == source_codec,
    };

    let mut plan = SelectionPlan::empty(format!("{}/{}", source.id, generic_selector(opts).0));
    plan.audio = Some(source.clone());
    plan.audio_conversion = Some(opts.audio_format);
    plan.transcodes_audio = !copies;
    plan.embed_thumbnail = opts.embed_thumbnail && opts.audio_format.supports_thumbnail();
    let src_abr = source.audio_bitrate().round() as u32;

    if copies {
        let (size, approx) = sum_sizes(&[Some(source)]);
        plan.estimated_size = size;
        plan.size_is_estimate = approx;
        plan.quality_label = format!("{} (original, no re-encoding)", describe_audio(source));
        if opts.audio_bitrate.is_some() {
            plan.note(NoteLevel::Info, "The source already matches the target format, so it is copied without re-encoding and the bitrate setting is not applied.");
        }
    } else {
        let target = opts.audio_format;
        plan.quality_label = match (target.is_lossless(), opts.audio_bitrate) {
            (true, _) => format!("{} from {}", target.label(), describe_audio(source)),
            (false, Some(kbps)) => format!(
                "{} {kbps} kbps from {}",
                target.label(),
                describe_audio(source)
            ),
            (false, None) => format!(
                "{} (highest quality) from {}",
                target.label(),
                describe_audio(source)
            ),
        };
        if let (Some(kbps), Some(secs)) = (opts.audio_bitrate, duration) {
            if !target.is_lossless() {
                plan.estimated_size = Some((secs * f64::from(kbps) * 1000.0 / 8.0) as u64);
                plan.size_is_estimate = true;
            }
        }
        if target.is_lossless() {
            plan.note(
                NoteLevel::Warning,
                format!("The source is lossy ({}). {} preserves it exactly but cannot restore detail that was already removed, and files will be much larger.", describe_audio(source), target.label()),
            );
        } else {
            plan.note(
                NoteLevel::Info,
                format!(
                    "Converting {} to {} re-encodes the audio.",
                    describe_audio(source),
                    target.label()
                ),
            );
            if let Some(kbps) = opts.audio_bitrate {
                if src_abr > 0 && kbps > src_abr + 16 {
                    plan.note(NoteLevel::Warning, format!("{kbps} kbps is higher than the {src_abr} kbps source. A higher bitrate will not improve quality."));
                }
            }
        }
    }
    if opts.embed_thumbnail && !opts.audio_format.supports_thumbnail() {
        plan.note(
            NoteLevel::Info,
            format!(
                "{} files cannot hold cover art, so the thumbnail is not embedded.",
                opts.audio_format.label()
            ),
        );
    }
    if source.has_video() {
        plan.note(
            NoteLevel::Info,
            "This source has no separate audio stream; audio is extracted from the video.",
        );
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(id: &str, codec: &str, w: u32, h: u32, fps: f64, tbr: f64) -> MediaFormat {
        MediaFormat {
            id: id.into(),
            ext: if codec.starts_with("avc") {
                "mp4".into()
            } else {
                "webm".into()
            },
            vcodec: Some(codec.into()),
            width: Some(w),
            height: Some(h),
            fps: Some(fps),
            tbr: Some(tbr),
            vbr: Some(tbr),
            filesize: Some((tbr * 1000.0) as u64),
            dynamic_range: Some("SDR".into()),
            ..Default::default()
        }
    }

    fn a(id: &str, codec: &str, abr: f64) -> MediaFormat {
        MediaFormat {
            id: id.into(),
            ext: if codec.starts_with("mp4a") {
                "m4a".into()
            } else {
                "webm".into()
            },
            acodec: Some(codec.into()),
            abr: Some(abr),
            tbr: Some(abr),
            filesize: Some((abr * 100.0) as u64),
            ..Default::default()
        }
    }

    fn youtube_like() -> Vec<MediaFormat> {
        let mut hdr = v("701", "vp09.02.51.10", 3840, 2160, 60.0, 30000.0);
        hdr.dynamic_range = Some("HDR10".into());
        vec![
            v("137", "avc1.640028", 1920, 1080, 30.0, 4000.0),
            v("299", "avc1.64002a", 1920, 1080, 60.0, 6000.0),
            v("303", "vp09.00.41.08", 1920, 1080, 60.0, 4500.0),
            v("315", "vp09.00.51.08", 3840, 2160, 60.0, 25000.0),
            v("401", "av01.0.13M.08", 3840, 2160, 60.0, 20000.0),
            hdr,
            v("22", "avc1.64001F", 1280, 720, 30.0, 2000.0),
            a("140", "mp4a.40.2", 129.0),
            a("251", "opus", 140.0),
            a("251-drc", "opus", 141.0),
            MediaFormat {
                id: "sb0".into(),
                ext: "mhtml".into(),
                ..Default::default()
            },
        ]
    }

    fn opts() -> DownloadOptions {
        DownloadOptions::default()
    }

    #[test]
    fn best_available_picks_highest_resolution_fps_and_hdr() {
        let plan = plan(&youtube_like(), Some(100.0), &opts()).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().id, "701");
        assert_eq!(plan.audio.as_ref().unwrap().id, "251", "non-DRC opus wins");
        assert!(plan.requires_merge);
        assert_eq!(plan.output_container, Some(Container::Webm));
        assert!(plan.format_selector.starts_with("701+251/"));
        assert!(plan.container_conflict.is_none());
        assert_eq!(plan.estimated_size, Some(30_000_000 + 14_000));
    }

    #[test]
    fn sdr_only_when_hdr_disabled() {
        let o = DownloadOptions {
            allow_hdr: false,
            ..opts()
        };
        let plan = plan(&youtube_like(), None, &o).unwrap();
        assert_eq!(
            plan.video.as_ref().unwrap().id,
            "401",
            "AV1 beats VP9 at equal resolution and fps"
        );
    }

    #[test]
    fn codec_preference_breaks_ties_but_never_lowers_resolution() {
        let o = DownloadOptions {
            allow_hdr: false,
            video_codec: VideoCodecPreference::Vp9,
            ..opts()
        };
        assert_eq!(
            plan(&youtube_like(), None, &o).unwrap().video.unwrap().id,
            "315"
        );
        let o = DownloadOptions {
            allow_hdr: false,
            video_codec: VideoCodecPreference::H264,
            ..opts()
        };
        let p = plan(&youtube_like(), None, &o).unwrap();
        assert_eq!(
            p.video.unwrap().resolution_key(),
            Some(2160),
            "H.264 preference does not drop 4K"
        );
    }

    #[test]
    fn exact_resolution_and_fps() {
        let o = DownloadOptions {
            quality_mode: QualityMode::Exact,
            target_height: Some(1080),
            ..opts()
        };
        assert_eq!(
            plan(&youtube_like(), None, &o).unwrap().video.unwrap().id,
            "303"
        );
        let o = DownloadOptions {
            quality_mode: QualityMode::Exact,
            target_height: Some(1080),
            target_fps: Some(30),
            ..opts()
        };
        assert_eq!(
            plan(&youtube_like(), None, &o).unwrap().video.unwrap().id,
            "137"
        );
        let o = DownloadOptions {
            quality_mode: QualityMode::Exact,
            target_height: Some(1440),
            ..opts()
        };
        assert_eq!(
            plan(&youtube_like(), None, &o).unwrap_err().code,
            ErrorCode::FormatUnavailable
        );
    }

    #[test]
    fn exact_requires_height() {
        let o = DownloadOptions {
            quality_mode: QualityMode::Exact,
            ..opts()
        };
        assert_eq!(
            plan(&youtube_like(), None, &o).unwrap_err().code,
            ErrorCode::InvalidInput
        );
    }

    #[test]
    fn compatible_mode_uses_h264_aac_mp4() {
        let o = DownloadOptions {
            quality_mode: QualityMode::Compatible,
            ..opts()
        };
        let p = plan(&youtube_like(), None, &o).unwrap();
        assert_eq!(p.video.as_ref().unwrap().id, "299");
        assert_eq!(p.audio.as_ref().unwrap().id, "140");
        assert_eq!(p.output_container, Some(Container::Mp4));
    }

    #[test]
    fn incompatible_container_is_offered_an_alternative() {
        let mut formats = youtube_like();
        formats.retain(|f| f.id == "299" || f.id == "140");
        let o = DownloadOptions {
            container: Container::Webm,
            ..opts()
        };
        let p = plan(&formats, None, &o).unwrap();
        let conflict = p.container_conflict.expect("conflict expected");
        assert_eq!(conflict.requested, Container::Webm);
        assert_eq!(conflict.suggested, Container::Mkv);
        assert_eq!(p.output_container, Some(Container::Mkv));
        assert_eq!(
            p.video.unwrap().id,
            "299",
            "quality is not reduced to fit the container"
        );
    }

    #[test]
    fn mp4_keeps_vp9_and_notes_compatibility() {
        let o = DownloadOptions {
            container: Container::Mp4,
            allow_hdr: false,
            video_codec: VideoCodecPreference::Vp9,
            ..opts()
        };
        let p = plan(&youtube_like(), None, &o).unwrap();
        assert!(p.container_conflict.is_none());
        assert_eq!(p.output_container, Some(Container::Mp4));
        assert_eq!(p.audio.unwrap().id, "251");
        assert!(p.notes.iter().any(|n| n.message.contains("older players")));
    }

    #[test]
    fn vertical_video_uses_short_side_for_resolution() {
        let formats = vec![
            v("h1080", "avc1", 1080, 1920, 30.0, 3000.0),
            v("h720", "avc1", 720, 1280, 30.0, 1500.0),
            a("aud", "mp4a.40.2", 128.0),
        ];
        let s = summarize(&formats);
        assert_eq!(
            s.resolutions.iter().map(|r| r.height).collect::<Vec<_>>(),
            vec![1080, 720]
        );
        let o = DownloadOptions {
            quality_mode: QualityMode::Exact,
            target_height: Some(720),
            ..opts()
        };
        assert_eq!(plan(&formats, None, &o).unwrap().video.unwrap().id, "h720");
    }

    #[test]
    fn progressive_only_sources_do_not_merge() {
        let mut muxed = v("hd", "h264", 1080, 1920, 30.0, 2500.0);
        muxed.acodec = Some("aac".into());
        let p = plan(&[muxed], None, &opts()).unwrap();
        assert!(!p.requires_merge);
        assert!(p.audio.is_none());
        assert_eq!(p.format_selector.split('/').next(), Some("hd"));
        assert_eq!(p.output_container, Some(Container::Mp4));
    }

    #[test]
    fn audio_original_copies_best_stream() {
        let o = DownloadOptions {
            media_type: MediaType::Audio,
            ..opts()
        };
        let p = plan(&youtube_like(), Some(200.0), &o).unwrap();
        assert_eq!(p.audio.as_ref().unwrap().id, "251");
        assert!(!p.transcodes_audio);
        assert!(p.video.is_none());
    }

    #[test]
    fn audio_m4a_prefers_native_aac_without_transcoding() {
        let o = DownloadOptions {
            media_type: MediaType::Audio,
            audio_format: AudioFormat::M4a,
            ..opts()
        };
        let p = plan(&youtube_like(), Some(200.0), &o).unwrap();
        assert_eq!(p.audio.as_ref().unwrap().id, "140");
        assert!(!p.transcodes_audio);
    }

    #[test]
    fn audio_mp3_transcodes_with_estimate() {
        let o = DownloadOptions {
            media_type: MediaType::Audio,
            audio_format: AudioFormat::Mp3,
            audio_bitrate: Some(192),
            ..opts()
        };
        let p = plan(&youtube_like(), Some(100.0), &o).unwrap();
        assert!(p.transcodes_audio);
        assert_eq!(p.estimated_size, Some(2_400_000));
        assert!(p.size_is_estimate);
    }

    #[test]
    fn lossless_targets_are_honest() {
        let o = DownloadOptions {
            media_type: MediaType::Audio,
            audio_format: AudioFormat::Flac,
            ..opts()
        };
        let p = plan(&youtube_like(), None, &o).unwrap();
        assert!(p.transcodes_audio);
        assert!(p
            .notes
            .iter()
            .any(|n| n.level == NoteLevel::Warning && n.message.contains("cannot restore")));
        let o = DownloadOptions {
            media_type: MediaType::Audio,
            audio_format: AudioFormat::Wav,
            ..opts()
        };
        let p = plan(&youtube_like(), None, &o).unwrap();
        assert!(!p.embed_thumbnail);
    }

    #[test]
    fn warns_about_upsampled_bitrate() {
        let o = DownloadOptions {
            media_type: MediaType::Audio,
            audio_format: AudioFormat::Mp3,
            audio_bitrate: Some(320),
            ..opts()
        };
        let p = plan(&youtube_like(), None, &o).unwrap();
        assert!(p
            .notes
            .iter()
            .any(|n| n.message.contains("will not improve")));
    }

    #[test]
    fn audio_from_video_only_sources() {
        let mut muxed = v("0", "h264", 576, 1024, 30.0, 1200.0);
        muxed.acodec = Some("aac".into());
        let o = DownloadOptions {
            media_type: MediaType::Audio,
            audio_format: AudioFormat::Mp3,
            ..opts()
        };
        let p = plan(&[muxed], None, &o).unwrap();
        assert_eq!(p.audio.unwrap().id, "0");
        assert!(p
            .notes
            .iter()
            .any(|n| n.message.contains("extracted from the video")));
    }

    #[test]
    fn custom_expressions_are_validated_and_resolved() {
        let o = DownloadOptions {
            quality_mode: QualityMode::Custom,
            custom_format: Some("137+140".into()),
            ..opts()
        };
        let p = plan(&youtube_like(), None, &o).unwrap();
        assert_eq!(p.format_selector, "137+140");
        assert_eq!(p.video.unwrap().id, "137");
        assert_eq!(p.audio.unwrap().id, "140");
        let bad = DownloadOptions {
            quality_mode: QualityMode::Custom,
            custom_format: Some("--exec x".into()),
            ..opts()
        };
        assert!(plan(&youtube_like(), None, &bad).is_err());
    }

    #[test]
    fn video_mode_fails_without_video() {
        let formats = vec![a("140", "mp4a.40.2", 128.0)];
        assert_eq!(
            plan(&formats, None, &opts()).unwrap_err().code,
            ErrorCode::FormatUnavailable
        );
    }

    #[test]
    fn summary_reports_real_options_only() {
        let s = summarize(&youtube_like());
        let heights: Vec<u32> = s.resolutions.iter().map(|r| r.height).collect();
        assert_eq!(heights, vec![2160, 1080, 720]);
        assert!(s.has_hdr);
        assert_eq!(s.max_fps, Some(60));
        assert_eq!(s.resolutions[1].frame_rates, vec![60, 30]);
        assert!(s.video_codecs.contains(&VideoCodec::Av1));
        assert_eq!(s.audio_bitrates, vec![141, 140, 129]);
        assert!(
            !heights.contains(&4320),
            "unavailable resolutions are never listed"
        );
    }

    #[test]
    fn generic_fallback_without_formats() {
        let p = plan(&[], None, &opts()).unwrap();
        assert!(p.format_selector.contains("bv*"));
        let o = DownloadOptions {
            media_type: MediaType::Audio,
            audio_format: AudioFormat::Opus,
            ..opts()
        };
        assert!(plan(&[], None, &o)
            .unwrap()
            .format_selector
            .starts_with("ba[acodec=opus]"));
    }

    #[test]
    fn container_rules() {
        assert!(container_supports(
            Container::Mp4,
            Some(VideoCodec::Av1),
            Some(AudioCodec::Aac)
        ));
        assert!(!container_supports(
            Container::Webm,
            Some(VideoCodec::H264),
            Some(AudioCodec::Opus)
        ));
        assert!(!container_supports(
            Container::Webm,
            Some(VideoCodec::Vp9),
            Some(AudioCodec::Aac)
        ));
        assert!(container_supports(
            Container::Mkv,
            Some(VideoCodec::Other),
            Some(AudioCodec::Other)
        ));
        assert_eq!(
            native_container(Some(VideoCodec::H264), Some(AudioCodec::Aac)),
            Container::Mp4
        );
        assert_eq!(
            native_container(Some(VideoCodec::Vp9), Some(AudioCodec::Opus)),
            Container::Webm
        );
        assert_eq!(
            native_container(Some(VideoCodec::H264), Some(AudioCodec::Opus)),
            Container::Mkv
        );
    }

    #[test]
    fn codec_parsing() {
        assert_eq!(
            VideoCodec::from_codec_string("avc1.640028"),
            VideoCodec::H264
        );
        assert_eq!(
            VideoCodec::from_codec_string("hev1.1.6.L93"),
            VideoCodec::H265
        );
        assert_eq!(
            VideoCodec::from_codec_string("vp09.00.40.08"),
            VideoCodec::Vp9
        );
        assert_eq!(
            VideoCodec::from_codec_string("av01.0.08M.08"),
            VideoCodec::Av1
        );
        assert_eq!(AudioCodec::from_codec_string("mp4a.40.2"), AudioCodec::Aac);
        assert_eq!(AudioCodec::from_codec_string("opus"), AudioCodec::Opus);
        assert_eq!(AudioCodec::from_codec_string("ec-3"), AudioCodec::Ac3);
    }
}
