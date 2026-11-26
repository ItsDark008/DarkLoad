# Development guide

## Prerequisites

| Tool | Version | Notes |
| --- | --- | --- |
| Node.js | 20 or later | 22 LTS is used in CI |
| pnpm | 12 | `npm i -g pnpm` or `corepack enable` |
| Rust | stable (1.85 or later) | install with [rustup](https://rustup.rs) |
| Windows | Visual Studio Build Tools with **Desktop development with C++**, WebView2 | MSVC is the supported toolchain |
| Linux | `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev patchelf build-essential` | Debian/Ubuntu package names |

Building with the `x86_64-pc-windows-gnu` toolchain and a MinGW-w64 GCC also works for local development; `build.rs` compiles the Windows manifest with `windres` in that case.

## Common tasks

```powershell
pnpm install                 # install frontend dependencies
pnpm tauri dev               # run the app with hot reload
pnpm typecheck               # TypeScript (strict)
pnpm lint                    # ESLint
pnpm test                    # Vitest unit tests
pnpm build                   # production frontend bundle
pnpm tauri build             # release binaries and installers
pnpm icons                   # regenerate icons from assets/brand/darkload-app-icon.svg
```

Rust checks run from `src-tauri`:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Architecture

DarkLoad is a Tauri 2 application. All download logic lives in Rust; the React frontend renders state and sends typed commands.

```
src/                         React + TypeScript (Vite, Tailwind CSS, Radix/shadcn-style UI, Zustand)
  components/ui/             accessible primitives (buttons, selects, dialogs, menus, switches)
  components/                feature components (job rows, options form, media previews)
  layouts/                   window chrome and sidebar
  pages/                     Overview, Downloader, Batch, Queue, History, Settings, About
  stores/                    Zustand stores (app, queue, downloader, batch)
  services/api.ts            typed IPC bridge — the only place `invoke` is called
  types/api.ts               TypeScript mirrors of the Rust data structures
src-tauri/src/
  platform.rs                URL validation, normalization and platform detection
  security.rs                filename sanitization, template validation, path checks, safe moves
  formats.rs                 format model, quality selection, container compatibility
  extractor.rs               yt-dlp JSON analysis and error classification
  download.rs                yt-dlp command construction and structured progress parsing
  converter.rs               FFmpeg progress-file parsing for merge/convert progress
  process.rs                 shell-free process spawning and process-tree cancellation
  queue.rs                   scheduler, workers, retries, pause/resume, persistence
  dependencies.rs            managed yt-dlp/FFmpeg/Deno: install, verify, update, diagnose
  storage.rs                 SQLite: settings, history, persistent queue
  settings.rs                settings schema, defaults and validation
  thumbnails.rs              local thumbnail cache served through the asset protocol
  updater.rs                 signed application updates (when configured)
  commands.rs / events.rs    typed IPC commands and Tauri events
src-tauri/examples/fake_ytdlp.rs   deterministic yt-dlp stand-in for queue tests
```

### Download pipeline

1. **Analyze** — `yt-dlp --dump-single-json` produces structured metadata. `formats.rs` summarizes the real resolutions, frame rates, codecs and audio bitrates.
2. **Plan** — the selected mode is turned into explicit format IDs (with a generic fallback expression) plus a container decision. Container conflicts are surfaced to the user.
3. **Download** — yt-dlp runs with `--progress-template` JSON lines, writing into a per-job working folder. Progress is aggregated across video and audio streams.
4. **Process** — FFmpeg writes machine-readable progress (`-progress file:…`), which the queue reads to report merge and conversion progress separately.
5. **Place** — the finished file is moved into the output folder according to the duplicate policy (keep both, skip or overwrite); the working folder is removed.

### Security model

- Capabilities in `src-tauri/capabilities/main.json` allow only the window, dialog, clipboard and opener operations the UI uses, and every custom command is opt-in (`build.rs` declares the command list).
- A strict Content Security Policy forbids remote scripts and remote images; thumbnails are fetched by the backend and served from the local cache.
- User input never reaches a shell. URLs are validated and normalized before use and passed after `--`; templates and format expressions are validated against allow-lists.
- Downloaded components are verified against published SHA-256 checksums before installation; archive entries are extracted by file name only.

## Tests

- **Rust:** unit tests cover URL validation, platform detection, format and quality selection, container rules, sanitization, Windows reserved names, Unicode paths, settings, storage, process cancellation, dependency checksum parsing and archive extraction. Queue tests drive real child processes through `examples/fake_ytdlp.rs` to verify progress, merging, conversion, duplicates, cancellation, pause, retries, concurrency limits and restart persistence.
- **Frontend:** Vitest covers formatting, URL extraction and de-duplication, color contrast and the queue, batch and downloader stores.

Real-network extraction is intentionally not part of CI. Use authorized media (for example Blender Foundation's openly licensed films) when testing manually.

## Data locations

| Data | Windows | Linux |
| --- | --- | --- |
| Database, managed tools | `%LOCALAPPDATA%\app.darkload.desktop` | `~/.local/share/app.darkload.desktop` |
| Thumbnails, temporary downloads | `%LOCALAPPDATA%\app.darkload.desktop` | `~/.cache/app.darkload.desktop` |
| Log file | `%LOCALAPPDATA%\app.darkload.desktop\logs` | `~/.local/share/app.darkload.desktop/logs` |
