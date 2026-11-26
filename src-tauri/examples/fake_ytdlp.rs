//! Deterministic stand-in for yt-dlp and FFmpeg used by DarkLoad's queue tests.
//!
//! It understands the subset of the command line DarkLoad generates and emits the same
//! structured progress lines the real tools produce. Behavior is selected by the video ID:
//!
//! * `ok…`      — succeeds quickly
//! * `slow…`    — downloads slowly (about 30 seconds), for cancellation tests
//! * `netfail…` — fails with a network error on the first attempt, then succeeds
//! * `private…` — fails with a private-video error

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn value_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn progress(format_id: &str, done: u64, total: u64, status: &str) {
    println!(
        "DLPROG {{\"f\":\"{format_id}\",\"p\":{{\"status\":\"{status}\",\"downloaded_bytes\":{done},\"total_bytes\":{total},\"total_bytes_estimate\":null,\"speed\":2048.0,\"eta\":1,\"fragment_index\":null,\"fragment_count\":null}}}}"
    );
    let _ = std::io::stdout().flush();
}

fn postprocess(name: &str, progress_file: Option<&Path>) {
    println!("DLPP {{\"status\":\"started\",\"postprocessor\":\"{name}\"}}");
    let _ = std::io::stdout().flush();
    if let Some(file) = progress_file {
        let _ = std::fs::write(
            file,
            "out_time_us=5000000\nspeed=10.0x\nprogress=continue\n",
        );
        std::thread::sleep(Duration::from_millis(600));
        let _ = std::fs::write(file, "out_time_us=10000000\nspeed=10.0x\nprogress=end\n");
    }
    println!("DLPP {{\"status\":\"finished\",\"postprocessor\":\"{name}\"}}");
    let _ = std::io::stdout().flush();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let program = Path::new(&args[0])
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if program.contains("ffmpeg") || program.contains("ffprobe") {
        println!("{program} version 7.1-fake Copyright (c) 2000-2026");
        return;
    }
    if args.iter().any(|a| a == "--version") {
        println!("2026.01.01");
        return;
    }
    let url = args.last().cloned().unwrap_or_default();
    let id = url.split("v=").nth(1).unwrap_or("unknown").to_string();

    if id.starts_with("private") {
        eprintln!("ERROR: [youtube] {id}: Private video. Sign in if you've been granted access to this video");
        std::process::exit(1);
    }

    if args.iter().any(|a| a == "--dump-single-json") {
        println!(
            "{{\"id\":\"{id}\",\"title\":\"Fake Video {id}\",\"uploader\":\"DarkLoad Tests\",\"duration\":10.0,\"webpage_url\":\"{url}\",\"formats\":[\
             {{\"format_id\":\"v1\",\"ext\":\"mp4\",\"vcodec\":\"avc1.640028\",\"acodec\":\"none\",\"width\":1280,\"height\":720,\"fps\":30,\"filesize\":4000}},\
             {{\"format_id\":\"a1\",\"ext\":\"m4a\",\"vcodec\":\"none\",\"acodec\":\"mp4a.40.2\",\"abr\":128,\"filesize\":1000}}]}}"
        );
        return;
    }

    let work = PathBuf::from(value_after(&args, "--paths").unwrap_or("."));
    let progress_file = value_after(&args, "--postprocessor-args")
        .and_then(|v| v.split("'file:").nth(1))
        .and_then(|v| v.split('\'').next())
        .map(PathBuf::from);

    if id.starts_with("netfail") {
        let marker = work.join(".fake-attempted");
        if !marker.exists() {
            let _ = std::fs::write(&marker, b"1");
            eprintln!("ERROR: unable to download video data: <urlopen error timed out>");
            std::process::exit(1);
        }
    }

    let audio = args.iter().any(|a| a == "--extract-audio");
    let ext = if audio {
        match value_after(&args, "--audio-format") {
            Some("best") | None => "m4a".to_string(),
            Some("vorbis") => "ogg".to_string(),
            Some(f) => f.to_string(),
        }
    } else {
        value_after(&args, "--merge-output-format")
            .or(value_after(&args, "--remux-video"))
            .unwrap_or("mp4")
            .to_string()
    };

    let (steps, delay) = if id.starts_with("slow") {
        (100u64, Duration::from_millis(300))
    } else {
        (5u64, Duration::from_millis(40))
    };
    // yt-dlp converts thumbnails before the media download starts.
    if args.iter().any(|a| a == "--convert-thumbnails") {
        postprocess("ThumbnailsConvertor", None);
    }
    let streams: &[(&str, u64)] = if audio {
        &[("a1", 1000)]
    } else {
        &[("v1", 4000), ("a1", 1000)]
    };
    for (format_id, total) in streams {
        let part = work.join(format!("{id}.f{format_id}.part"));
        for step in 1..=steps {
            let done = total * step / steps;
            let _ = std::fs::write(&part, vec![0u8; done as usize]);
            progress(format_id, done, *total, "downloading");
            std::thread::sleep(delay);
        }
        progress(format_id, *total, *total, "finished");
        let _ = std::fs::remove_file(&part);
    }
    if audio {
        postprocess("ExtractAudio", progress_file.as_deref());
    } else {
        postprocess("Merger", progress_file.as_deref());
    }
    postprocess("Metadata", None);
    let final_path = work.join(format!("Fake Video {id} [{id}].{ext}"));
    std::fs::write(&final_path, vec![1u8; 5000]).expect("write output");
    println!("DLFILE {}", json_string(&final_path.to_string_lossy()));
}
