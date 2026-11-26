//! FFmpeg processing progress.
//!
//! yt-dlp runs FFmpeg for merging and conversion. DarkLoad asks FFmpeg to write machine-readable
//! progress (`-progress file:<path>`) and reads the tail of that file to report real merge and
//! conversion progress, measured against the media duration.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Parsed FFmpeg progress snapshot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FfmpegProgress {
    /// Processed media time in seconds.
    pub out_time: f64,
    /// Processing speed relative to real time (for example `12.5` for 12.5x).
    pub speed: Option<f64>,
    pub ended: bool,
}

/// Parses the last complete progress block of FFmpeg `-progress` output.
pub fn parse(text: &str) -> Option<FfmpegProgress> {
    let mut out_time: Option<f64> = None;
    let mut speed: Option<f64> = None;
    let mut ended = false;
    let mut current_time: Option<f64> = None;
    let mut current_speed: Option<f64> = None;
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key {
            // `out_time_ms` is (despite its name) also in microseconds.
            "out_time_us" | "out_time_ms" => {
                if let Ok(us) = value.parse::<i64>() {
                    if us >= 0 {
                        current_time = Some(us as f64 / 1_000_000.0);
                    }
                }
            }
            "speed" => {
                current_speed = value
                    .trim_end_matches('x')
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|s| s.is_finite() && *s > 0.0)
            }
            "progress" => {
                if current_time.is_some() {
                    out_time = current_time;
                }
                speed = current_speed.or(speed);
                ended = value == "end";
            }
            _ => {}
        }
    }
    out_time.map(|out_time| FfmpegProgress {
        out_time,
        speed,
        ended,
    })
}

/// Reads the tail of a progress file and parses it.
pub fn read_progress_file(path: &Path) -> Option<FfmpegProgress> {
    const TAIL: u64 = 8 * 1024;
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len > TAIL {
        file.seek(SeekFrom::Start(len - TAIL)).ok()?;
    }
    let mut buf = Vec::with_capacity(TAIL as usize);
    file.take(TAIL).read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    // Drop a possibly truncated first line when reading from the middle of the file.
    let text = if len > TAIL {
        text.split_once('\n').map(|(_, rest)| rest).unwrap_or("")
    } else {
        &text
    };
    parse(text)
}

/// Fraction complete for a processing step, given the media duration.
pub fn fraction(progress: &FfmpegProgress, duration: Option<f64>) -> Option<f64> {
    if progress.ended {
        return Some(1.0);
    }
    let duration = duration.filter(|d| *d > 0.0)?;
    Some((progress.out_time / duration).clamp(0.0, 0.999))
}

/// Remaining time estimate in seconds for a processing step.
pub fn eta(progress: &FfmpegProgress, duration: Option<f64>) -> Option<f64> {
    let duration = duration.filter(|d| *d > 0.0)?;
    let speed = progress.speed?;
    Some(((duration - progress.out_time).max(0.0)) / speed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "frame=120\nfps=0.0\nout_time_us=2000000\nout_time_ms=2000000\nout_time=00:00:02.000000\nspeed=4.0x\nprogress=continue\nframe=480\nout_time_us=8000000\nout_time_ms=8000000\nspeed=8.00x\nprogress=continue\n";

    #[test]
    fn parses_latest_block() {
        let p = parse(SAMPLE).unwrap();
        assert_eq!(p.out_time, 8.0);
        assert_eq!(p.speed, Some(8.0));
        assert!(!p.ended);
        assert_eq!(fraction(&p, Some(16.0)), Some(0.5));
        assert_eq!(eta(&p, Some(16.0)), Some(1.0));
    }

    #[test]
    fn detects_end() {
        let p = parse("out_time_us=1000\nspeed=N/A\nprogress=end\n").unwrap();
        assert!(p.ended);
        assert_eq!(fraction(&p, None), Some(1.0));
    }

    #[test]
    fn ignores_incomplete_blocks_and_garbage() {
        assert!(parse("").is_none());
        assert!(parse("out_time_us=N/A\nprogress=continue\n").is_none());
        // An unterminated block is not reported until FFmpeg writes `progress=`.
        let p = parse("out_time_us=1000000\nprogress=continue\nout_time_us=9000000\n").unwrap();
        assert_eq!(p.out_time, 1.0);
    }

    #[test]
    fn unknown_duration_has_no_fraction() {
        let p = parse(SAMPLE).unwrap();
        assert_eq!(fraction(&p, None), None);
    }

    #[test]
    fn reads_tail_of_large_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("progress.txt");
        let mut content = String::new();
        for i in 0..2000 {
            content.push_str(&format!(
                "frame={i}\nout_time_us={}\nspeed=2.0x\nprogress=continue\n",
                i * 1000
            ));
        }
        content.push_str("out_time_us=5000000\nprogress=end\n");
        std::fs::write(&path, content).unwrap();
        let p = read_progress_file(&path).unwrap();
        assert_eq!(p.out_time, 5.0);
        assert!(p.ended);
        assert!(read_progress_file(&dir.path().join("missing")).is_none());
    }
}
