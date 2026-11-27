//! yt-dlp download command construction and structured progress parsing.
//!
//! Progress is read from machine-readable templates (`--progress-template` with JSON
//! conversions and `--print` with JSON), never from human-readable log lines.

use std::ffi::OsString;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::extractor::{base_args, NetOptions, ToolPaths};
use crate::formats::DownloadOptions;
use crate::formats::{AudioFormat, SelectionPlan};
use crate::settings::{MediaType, Settings, DEFAULT_TEMPLATE};

pub const PROGRESS_PREFIX: &str = "DLPROG ";
pub const POSTPROCESS_PREFIX: &str = "DLPP ";
pub const FILE_PREFIX: &str = "DLFILE ";

/// Everything needed to start one yt-dlp download.
pub struct DownloadSpec<'a> {
    pub url: &'a str,
    pub playlist_item: Option<u32>,
    pub options: &'a DownloadOptions,
    pub plan: &'a SelectionPlan,
    pub settings: &'a Settings,
    pub work_dir: &'a Path,
    pub ffmpeg_progress_file: Option<&'a Path>,
    /// Temporary Netscape cookie file for this operation.
    pub cookies_file: Option<&'a Path>,
    /// Sanitized media information resolved by the public TikTok player.
    pub info_file: Option<&'a Path>,
}

/// Quotes a value for yt-dlp's shlex-based `--postprocessor-args` parser.
fn shlex_quote(value: &str) -> Option<String> {
    if value.contains('\'') {
        return None;
    }
    Some(format!("'{value}'"))
}

pub fn build_args(tools: &ToolPaths, spec: &DownloadSpec<'_>) -> Vec<OsString> {
    let s = spec.settings;
    let o = spec.options;
    let mut args = base_args(tools, NetOptions::from(s));
    let template = o.filename_template.clone().unwrap_or_else(|| {
        if s.filename_template.trim().is_empty() {
            DEFAULT_TEMPLATE.to_string()
        } else {
            s.filename_template.clone()
        }
    });

    args.extend::<Vec<OsString>>(vec![
        "--newline".into(),
        "--progress".into(),
        "--no-simulate".into(),
        "--quiet".into(),
        "--no-warnings".into(),
        "--continue".into(),
        "--no-mtime".into(),
        "--windows-filenames".into(),
        "--trim-filenames".into(),
        "180".into(),
        "--retries".into(),
        s.retry_attempts.max(1).to_string().into(),
        "--fragment-retries".into(),
        s.retry_attempts.max(1).to_string().into(),
        "--concurrent-fragments".into(),
        s.concurrent_fragments.to_string().into(),
        "--paths".into(),
        spec.work_dir.as_os_str().to_owned(),
        "--output".into(),
        template.into(),
        "--progress-template".into(),
        format!(
            "download:{PROGRESS_PREFIX}{{\"f\":%(info.format_id)j,\"p\":%(progress.{{status,downloaded_bytes,total_bytes,total_bytes_estimate,speed,eta,fragment_index,fragment_count}})j}}"
        )
        .into(),
        "--progress-template".into(),
        format!("postprocess:{POSTPROCESS_PREFIX}%(progress.{{status,postprocessor}})j").into(),
        "--print".into(),
        format!("after_move:{FILE_PREFIX}%(filepath)j").into(),
        "--format".into(),
        spec.plan.format_selector.clone().into(),
    ]);

    if let Some(sort) = &spec.plan.format_sort {
        args.push("--format-sort".into());
        args.push(sort.clone().into());
    }
    if let Some(limit) = s.speed_limit_kib {
        args.push("--limit-rate".into());
        args.push(format!("{limit}K").into());
    }
    match spec.playlist_item {
        Some(item) => {
            args.push("--yes-playlist".into());
            args.push("--playlist-items".into());
            args.push(item.to_string().into());
        }
        None => args.push("--no-playlist".into()),
    }

    match o.media_type {
        MediaType::Video => {
            if !o.audio_tracks.is_auto() && spec.plan.audio_count() > 1 {
                args.push("--audio-multistreams".into());
            }
            if let Some(container) = spec.plan.output_container.and_then(|c| c.ext()) {
                if spec.plan.requires_merge {
                    args.push("--merge-output-format".into());
                    args.push(container.into());
                } else {
                    // Single pre-muxed stream: change the container without re-encoding.
                    args.push("--remux-video".into());
                    args.push(container.into());
                }
            }
            if o.download_subtitles {
                args.push("--write-subs".into());
                if o.include_auto_subtitles {
                    args.push("--write-auto-subs".into());
                }
                args.push("--sub-langs".into());
                args.push(o.subtitle_languages.join(",").into());
                if spec.plan.embed_subtitles {
                    args.push("--embed-subs".into());
                }
            }
            if o.embed_chapters {
                args.push("--embed-chapters".into());
            }
        }
        MediaType::Audio => {
            let target = spec.plan.audio_conversion.unwrap_or(AudioFormat::Original);
            args.push("--extract-audio".into());
            args.push("--audio-format".into());
            args.push(target.ytdlp_name().into());
            let quality = match (target.is_lossless(), o.audio_bitrate) {
                (false, Some(kbps)) => format!("{kbps}K"),
                _ => "0".to_string(),
            };
            args.push("--audio-quality".into());
            args.push(quality.into());
            if o.embed_chapters {
                args.push("--embed-chapters".into());
            }
        }
    }

    if o.embed_metadata {
        args.push("--embed-metadata".into());
    }
    if spec.plan.embed_thumbnail {
        args.push("--embed-thumbnail".into());
        args.push("--convert-thumbnails".into());
        args.push("jpg".into());
    }
    if o.write_thumbnail {
        args.push("--write-thumbnail".into());
    }

    if let Some(file) = spec.cookies_file {
        args.push("--cookies".into());
        args.push(file.as_os_str().to_owned());
    }

    if let Some(progress_file) = spec.ffmpeg_progress_file {
        if let Some(quoted) = shlex_quote(&format!("file:{}", progress_file.display())) {
            args.push("--postprocessor-args".into());
            args.push(format!("ffmpeg:-progress {quoted} -nostats").into());
        }
    }

    if let Some(file) = spec.info_file {
        args.push("--load-info-json".into());
        args.push(file.as_os_str().to_owned());
    } else {
        args.push("--".into());
        args.push(spec.url.into());
    }
    args
}

/// Raw progress values from the download template.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct RawProgress {
    pub status: Option<String>,
    pub downloaded_bytes: Option<f64>,
    pub total_bytes: Option<f64>,
    pub total_bytes_estimate: Option<f64>,
    pub speed: Option<f64>,
    pub eta: Option<f64>,
    pub fragment_index: Option<f64>,
    pub fragment_count: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Progress {
        format_id: Option<String>,
        progress: RawProgress,
    },
    PostProcess {
        status: String,
        postprocessor: String,
    },
    File(String),
}

/// Parses one line of yt-dlp output into a structured event.
pub fn parse_line(line: &str) -> Option<Event> {
    let line = line.trim_start();
    if let Some(json) = line.strip_prefix(PROGRESS_PREFIX) {
        let v: Value = serde_json::from_str(json).ok()?;
        let format_id = v.get("f").and_then(Value::as_str).map(str::to_string);
        let progress: RawProgress = v
            .get("p")
            .cloned()
            .and_then(|p| serde_json::from_value(p).ok())
            .unwrap_or_default();
        return Some(Event::Progress {
            format_id,
            progress,
        });
    }
    if let Some(json) = line.strip_prefix(POSTPROCESS_PREFIX) {
        let v: Value = serde_json::from_str(json).ok()?;
        return Some(Event::PostProcess {
            status: v
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            postprocessor: v
                .get("postprocessor")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        });
    }
    if let Some(json) = line.strip_prefix(FILE_PREFIX) {
        let path: String = serde_json::from_str(json).ok()?;
        return Some(Event::File(path));
    }
    None
}

/// Whether a post-processor is a conversion (re-encode) rather than a merge or metadata step.
pub fn is_conversion(postprocessor: &str) -> bool {
    matches!(
        postprocessor,
        "ExtractAudio"
            | "VideoConvertor"
            | "VideoRemuxer"
            | "FFmpegExtractAudio"
            | "FFmpegVideoConvertor"
            | "FFmpegVideoRemuxer"
    )
}

pub fn is_merge(postprocessor: &str) -> bool {
    postprocessor == "Merger" || postprocessor == "FFmpegMerger"
}

/// Post-processors that yt-dlp runs before the media download (`before_dl`).
pub fn runs_before_download(postprocessor: &str) -> bool {
    matches!(
        postprocessor.trim_start_matches("FFmpeg"),
        "ThumbnailsConvertor" | "SubtitlesConvertor"
    )
}

/// Human-readable label for a post-processing step.
pub fn postprocessor_label(postprocessor: &str) -> &'static str {
    match postprocessor.trim_start_matches("FFmpeg") {
        "Merger" => "Merging streams",
        "ExtractAudio" => "Converting audio",
        "VideoConvertor" => "Converting video",
        "VideoRemuxer" => "Remuxing",
        "EmbedThumbnail" => "Embedding thumbnail",
        "Metadata" => "Writing metadata",
        "EmbedSubtitle" => "Embedding subtitles",
        "SubtitlesConvertor" => "Converting subtitles",
        "ThumbnailsConvertor" => "Converting thumbnail",
        "FixupM3u8" | "FixupM4a" | "FixupStretched" | "FixupDuplicateMoov" | "FixupTimestamp"
        | "FixupDuration" => "Fixing container",
        "MoveFiles" => "Finalizing",
        _ => "Processing",
    }
}

/// Aggregates per-stream progress into overall download progress.
#[derive(Debug, Clone, Default)]
pub struct ProgressTracker {
    streams: Vec<(String, f64, Option<f64>, bool)>,
    pub expected_streams: usize,
    pub expected_sizes: Vec<Option<u64>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Aggregate {
    pub fraction: Option<f64>,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed: Option<f64>,
    pub eta: Option<f64>,
}

impl ProgressTracker {
    pub fn new(expected_streams: usize, expected_sizes: Vec<Option<u64>>) -> Self {
        Self {
            streams: Vec::new(),
            expected_streams: expected_streams.max(1),
            expected_sizes,
        }
    }

    pub fn update(&mut self, format_id: Option<&str>, p: &RawProgress) -> Aggregate {
        let id = format_id.unwrap_or("").to_string();
        let downloaded = p.downloaded_bytes.unwrap_or(0.0).max(0.0);
        let mut total = p
            .total_bytes
            .or(p.total_bytes_estimate)
            .filter(|t| *t > 0.0);
        if total.is_none() {
            if let (Some(i), Some(c)) = (p.fragment_index, p.fragment_count) {
                if c > 0.0 && i > 0.0 && downloaded > 0.0 {
                    total = Some(downloaded / (i / c).min(1.0));
                }
            }
        }
        let finished = p.status.as_deref() == Some("finished");
        match self.streams.iter_mut().find(|s| s.0 == id) {
            Some(s) => {
                s.1 = if finished {
                    s.2.unwrap_or(downloaded).max(downloaded)
                } else {
                    downloaded
                };
                s.2 = total.or(s.2);
                s.3 = finished;
            }
            None => self.streams.push((id, downloaded, total, finished)),
        }
        let downloaded_sum: f64 = self.streams.iter().map(|s| s.1).sum();
        let expected = self.expected_streams.max(self.streams.len());

        // Total: known stream totals plus planned sizes for streams that have not started.
        let mut total_sum = 0.0;
        let mut total_known = true;
        for s in &self.streams {
            match s.2 {
                Some(t) => total_sum += t.max(s.1),
                None => total_known = false,
            }
        }
        for i in self.streams.len()..expected {
            match self.expected_sizes.get(i).copied().flatten() {
                Some(size) => total_sum += size as f64,
                None => total_known = false,
            }
        }
        let fraction = if total_known && total_sum > 0.0 {
            Some((downloaded_sum / total_sum).clamp(0.0, 1.0))
        } else {
            // Fall back to stream-weighted progress when sizes are unknown.
            let mut acc = 0.0;
            let mut any = false;
            for s in &self.streams {
                if s.3 {
                    acc += 1.0;
                    any = true;
                } else if let Some(t) = s.2 {
                    acc += (s.1 / t).clamp(0.0, 1.0);
                    any = true;
                }
            }
            any.then(|| (acc / expected as f64).clamp(0.0, 1.0))
        };
        Aggregate {
            fraction,
            downloaded: downloaded_sum as u64,
            total: total_known.then_some(total_sum as u64).filter(|t| *t > 0),
            speed: if finished {
                None
            } else {
                p.speed.filter(|s| *s > 0.0)
            },
            eta: if finished {
                None
            } else {
                p.eta.filter(|e| *e >= 0.0)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::{plan, AudioTrackSelection, Container, MediaFormat};

    fn tools() -> ToolPaths {
        ToolPaths {
            yt_dlp: "yt-dlp".into(),
            ffmpeg: Some("/opt/ffmpeg".into()),
            deno: None,
            cache_dir: "/cache".into(),
        }
    }

    fn args_for(options: &DownloadOptions, settings: &Settings) -> Vec<String> {
        args_with_formats(options, settings, &[])
    }

    fn args_with_formats(
        options: &DownloadOptions,
        settings: &Settings,
        formats: &[MediaFormat],
    ) -> Vec<String> {
        let plan = plan(formats, None, options).unwrap();
        let spec = DownloadSpec {
            url: "https://www.youtube.com/watch?v=aqz-KE-bpKQ",
            playlist_item: None,
            options,
            plan: &plan,
            settings,
            work_dir: Path::new("/work"),
            ffmpeg_progress_file: Some(Path::new("/work/ffmpeg-progress.txt")),
            cookies_file: None,
            info_file: None,
        };
        build_args(&tools(), &spec)
            .into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    fn has_pair(args: &[String], flag: &str, value: &str) -> bool {
        args.windows(2).any(|w| w[0] == flag && w[1] == value)
    }

    #[test]
    fn url_is_last_and_separated() {
        let args = args_for(&DownloadOptions::default(), &Settings::default());
        let n = args.len();
        assert_eq!(args[n - 2], "--");
        assert_eq!(args[n - 1], "https://www.youtube.com/watch?v=aqz-KE-bpKQ");
    }

    #[test]
    fn video_args() {
        let o = DownloadOptions {
            container: Container::Mkv,
            download_subtitles: true,
            subtitle_languages: vec!["en".into(), "de".into()],
            ..Default::default()
        };
        let s = Settings {
            speed_limit_kib: Some(500),
            ..Settings::default()
        };
        let args = args_for(&o, &s);
        assert!(has_pair(&args, "--limit-rate", "500K"));
        assert!(has_pair(&args, "--sub-langs", "en,de"));
        assert!(args.contains(&"--embed-subs".to_string()));
        assert!(args.contains(&"--embed-metadata".to_string()));
        assert!(args.contains(&"--embed-thumbnail".to_string()));
        assert!(args.contains(&"--no-playlist".to_string()));
        assert!(has_pair(&args, "--paths", "/work"));
        assert!(has_pair(&args, "--ffmpeg-location", "/opt/ffmpeg"));
        assert!(has_pair(
            &args,
            "--postprocessor-args",
            "ffmpeg:-progress 'file:/work/ffmpeg-progress.txt' -nostats"
        ));
        assert!(!args.contains(&"--extract-audio".to_string()));
        assert!(!args.contains(&"--audio-multistreams".to_string()));
    }

    #[test]
    fn video_with_selected_languages_enables_multiple_audio_streams() {
        let formats = [
            MediaFormat {
                id: "v".into(),
                vcodec: Some("avc1".into()),
                ..Default::default()
            },
            MediaFormat {
                id: "eng".into(),
                language: Some("en".into()),
                acodec: Some("mp4a".into()),
                ..Default::default()
            },
            MediaFormat {
                id: "ita".into(),
                language: Some("it".into()),
                acodec: Some("mp4a".into()),
                ..Default::default()
            },
        ];
        let options = DownloadOptions {
            audio_tracks: AudioTrackSelection {
                original: false,
                languages: vec!["en".into(), "it".into()],
            },
            ..Default::default()
        };
        let args = args_with_formats(&options, &Settings::default(), &formats);
        assert!(args.contains(&"--audio-multistreams".to_string()));
        assert!(has_pair(
            &args,
            "--format",
            "v+eng[language=en]+ita[language=it]"
        ));
        assert!(has_pair(&args, "--merge-output-format", "mp4"));
    }

    #[test]
    fn audio_args() {
        let o = DownloadOptions {
            media_type: MediaType::Audio,
            audio_format: AudioFormat::Mp3,
            audio_bitrate: Some(256),
            ..Default::default()
        };
        let args = args_for(&o, &Settings::default());
        assert!(args.contains(&"--extract-audio".to_string()));
        assert!(has_pair(&args, "--audio-format", "mp3"));
        assert!(has_pair(&args, "--audio-quality", "256K"));
        let o = DownloadOptions {
            media_type: MediaType::Audio,
            audio_format: AudioFormat::Flac,
            audio_bitrate: Some(256),
            ..Default::default()
        };
        let args = args_for(&o, &Settings::default());
        assert!(has_pair(&args, "--audio-quality", "0"));
        let o = DownloadOptions {
            media_type: MediaType::Audio,
            audio_format: AudioFormat::Wav,
            ..Default::default()
        };
        assert!(!args_for(&o, &Settings::default()).contains(&"--embed-thumbnail".to_string()));
    }

    #[test]
    fn quotes_are_never_injected_into_postprocessor_args() {
        assert_eq!(shlex_quote("a b"), Some("'a b'".into()));
        assert_eq!(shlex_quote("it's"), None);
    }

    #[test]
    fn parses_progress_lines() {
        let line = r#"DLPROG {"f":"401","p":{"status":"downloading","downloaded_bytes":1048576,"total_bytes":null,"total_bytes_estimate":4194304.0,"speed":524288.5,"eta":6,"fragment_index":3,"fragment_count":12}}"#;
        let Some(Event::Progress {
            format_id,
            progress,
        }) = parse_line(line)
        else {
            panic!()
        };
        assert_eq!(format_id.as_deref(), Some("401"));
        assert_eq!(progress.downloaded_bytes, Some(1_048_576.0));
        assert_eq!(progress.total_bytes, None);
        assert_eq!(progress.eta, Some(6.0));
        assert_eq!(
            parse_line(r#"DLPP {"status":"started","postprocessor":"Merger"}"#),
            Some(Event::PostProcess {
                status: "started".into(),
                postprocessor: "Merger".into()
            })
        );
        assert_eq!(
            parse_line(r#"DLFILE "C:\\Users\\Me\\Видео\\clip.mp4""#),
            Some(Event::File(r"C:\Users\Me\Видео\clip.mp4".into()))
        );
        assert_eq!(parse_line("[download] 10% of 5MiB"), None);
        assert_eq!(parse_line("DLPROG not json"), None);
    }

    #[test]
    fn aggregates_two_streams_with_known_sizes() {
        let mut t = ProgressTracker::new(2, vec![Some(800), Some(200)]);
        let p = |d: f64, total: f64, status: &str| RawProgress {
            status: Some(status.into()),
            downloaded_bytes: Some(d),
            total_bytes: Some(total),
            speed: Some(10.0),
            eta: Some(1.0),
            ..Default::default()
        };
        let a = t.update(Some("v"), &p(400.0, 800.0, "downloading"));
        assert_eq!(a.fraction, Some(0.4));
        assert_eq!(a.total, Some(1000));
        t.update(Some("v"), &p(800.0, 800.0, "finished"));
        let a = t.update(Some("a"), &p(100.0, 200.0, "downloading"));
        assert_eq!(a.fraction, Some(0.9));
        assert_eq!(a.downloaded, 900);
    }

    #[test]
    fn aggregates_with_unknown_sizes() {
        let mut t = ProgressTracker::new(2, vec![None, None]);
        let a = t.update(
            Some("v"),
            &RawProgress {
                downloaded_bytes: Some(50.0),
                total_bytes_estimate: Some(100.0),
                ..Default::default()
            },
        );
        assert_eq!(a.fraction, Some(0.25));
        assert_eq!(a.total, None);
        let a = t.update(
            Some("v"),
            &RawProgress {
                status: Some("finished".into()),
                downloaded_bytes: Some(100.0),
                ..Default::default()
            },
        );
        assert_eq!(a.fraction, Some(0.5));
    }

    #[test]
    fn estimates_total_from_fragments() {
        let mut t = ProgressTracker::new(1, vec![None]);
        let a = t.update(
            Some("hls"),
            &RawProgress {
                downloaded_bytes: Some(250.0),
                fragment_index: Some(1.0),
                fragment_count: Some(4.0),
                ..Default::default()
            },
        );
        assert_eq!(a.fraction, Some(0.25));
    }

    #[test]
    fn postprocessor_classification() {
        assert!(is_merge("Merger"));
        assert!(runs_before_download("ThumbnailsConvertor"));
        assert!(runs_before_download("FFmpegSubtitlesConvertor"));
        assert!(!runs_before_download("Merger"));
        assert!(is_conversion("ExtractAudio"));
        assert!(!is_conversion("Merger"));
        assert_eq!(postprocessor_label("FFmpegMerger"), "Merging streams");
        assert_eq!(postprocessor_label("EmbedThumbnail"), "Embedding thumbnail");
    }
}
