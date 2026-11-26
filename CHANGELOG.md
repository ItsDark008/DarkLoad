# Changelog

All notable changes to DarkLoad are documented in this file.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.0.0] - 2026-11-26

### Added

- Desktop application built with Tauri 2, Rust and React with a pure-black OLED interface and a collapsible sidebar. Settings and About sit at the bottom of the sidebar.
- Customizable OLED background color with six presets and support for sufficiently dark custom colors.
- Link analysis for YouTube videos, Shorts, playlists and archived livestreams, public TikTok videos and public Instagram reels, posts and carousels. TikTok and Instagram use guest access, with a public embedded-player fallback for TikTok.
- Quality selection that inspects the formats offered by the source: Best Available, Exact Resolution, Best Compatible and Custom expressions, with container compatibility checks that never silently reduce quality.
- Audio extraction to Original, MP3, M4A, AAC, FLAC, WAV, Opus and OGG with configurable bitrate and honest notes about lossy sources.
- Metadata, thumbnail, chapter and subtitle embedding, filename templates, duplicate handling and speed limits.
- Batch downloads from pasted text, imported files or drag and drop, with duplicate removal and per-item settings.
- Asynchronous download queue with configurable concurrency, real download, merge and conversion progress, pause and resume, cancellation, automatic retries and persistence across restarts.
- SQLite download history with search, filters, sorting, re-download and safe removal.
- Managed yt-dlp, FFmpeg, FFprobe and Deno components installed from official releases with SHA-256 verification.
- Desktop notifications, optional clipboard monitoring and signed application updates for configured builds.
- A "Prefer IPv4" setting, enabled by default, to prevent analysis from stalling on networks with broken IPv6 connectivity.

### Fixed

- Thumbnail conversion before a download preserves the download status.
- The "already downloaded" warning only appears for a previous download of the same media type.

### Security

- Updated the bundled SQLite (rusqlite 0.43) and the zip extraction library (zip 9) to address published advisories.
