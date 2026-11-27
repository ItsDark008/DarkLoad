# Changelog

All notable changes to DarkLoad are documented in this file.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.4.1] - 2026-11-27

### Fixed

- YouTube Browser on Linux: the browser now sits in its place on the Browser page instead of below a squashed window, and pages load normally instead of showing placeholders forever.
- YouTube Browser on Linux: the page no longer freezes when a video or preview starts playing. The AppImage now bundles GStreamer, and the `.deb` and `.rpm` packages install GStreamer's good plugins.
- YouTube Browser on Linux: clicking the label of "Select for DarkLoad" now selects the video.

## [1.4.0] - 2026-11-27

### Added

- **Italian language.** DarkLoad is now available in Italian. Choose the language in Settings → General → Language, or keep **System default** to follow your operating system. Menus, messages, errors, desktop notifications and the YouTube Browser controls are translated, and dates and language names follow the chosen language.

## [1.3.0] - 2026-11-27

### Added

- **Download presets.** Save quality, format, subtitles, audio tracks and destination so you can reuse the same setup across download screens. The menu shows the active preset, and **Originale** restores the defaults.
- **Audio track selection.** Save the original audio, choose an available language, or keep multiple tracks in one video so you can switch between them during playback.

## [1.2.0] - 2026-11-27

### Added

- YouTube Browser: browse YouTube inside DarkLoad and pick videos as you go. Hover a thumbnail on the home page, search results, channels, playlists, related videos or Shorts and tick its box, or open a video and choose "Select this video". Picks gather in a side panel with their title, channel and duration; download them all at once with the usual quality and format settings. Playlists and videos can also be sent to the Downloader for a full analysis.
- The browser runs in a private session that keeps nothing after DarkLoad closes, stays signed out and only opens YouTube pages. Links that leave YouTube can be opened in the system browser instead.

### Changed

- The keyboard shortcuts for switching pages now run from Ctrl+1 to Ctrl+8, with the YouTube Browser at Ctrl+4.

### Fixed

- Thumbnails no longer fall back to a placeholder on Windows when DarkLoad is started by a packaged (MSIX) app, which stores new files in a redirected folder.

## [1.1.0] - 2026-11-27

### Added

- Spotify songs, albums and playlists (`open.spotify.com` links, `spotify:` URIs and `spotify.link` short links). DarkLoad reads the song metadata from Spotify, finds the same recording on YouTube by title, artist and duration, downloads that audio and tags the file with Spotify's title, artists, album, track number, release date and cover. Spotify's own DRM-protected audio is never downloaded.
- The queue shows which YouTube upload was used as the audio source for each Spotify song. Songs without a reliable match fail with a clear message instead of downloading something else.
- Optional Spotify API credentials (Settings → Downloads) to read playlists with more than 100 songs and album names for single songs.
- Automatic application updates. DarkLoad checks at startup and every 6 hours, downloads a new version in the background, verifies its signature and installs it when DarkLoad closes, or immediately with "Restart now". Settings → Updates offers "Download and install", "Only notify me" and "Off". Portable copies only announce new versions.
- The update manifest lists a package per installation type (NSIS, MSI, AppImage, deb and rpm), so every copy updates with the installer it was installed with.

### Changed

- The "Check for updates automatically" switch became the update mode setting; turning it off in 1.0 carries over as "Off".
- DarkLoad is now credited to ItsDark008 in the license, the About page and the application metadata.

### Notes

- Version 1.0.0 was published without an update channel, so it cannot update itself. Install the next version once manually; later versions update automatically.

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
